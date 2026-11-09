use super::*;
use crate::board::{CanvasObjectId, Card, CardKind, InkStroke, InkTool, ShapeKind, SheetRootMode};
use crate::geometry::Color;
use crate::layout::{ApproxMeasure, layout_sheet};
use crate::model::Sheet;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h))
}

fn scene(s: &Sheet) -> (Layout, Scene) {
    let layout = layout_sheet(s, &ApproxMeasure, false);
    let scene = build_scene(s, &layout);
    (layout, scene)
}

fn shape(s: &mut Sheet, kind: ShapeKind, r: Rect, band: Band) -> CanvasObjectId {
    s.add_canvas_object(CanvasObject {
        band,
        ..CanvasObject::shape(kind, r)
    })
    .unwrap()
}

#[test]
fn fixed_connector_ports_follow_shape_geometry_for_every_route() {
    let mut sheet = Sheet::new("Ports", "Root");
    let source = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(100.0, 100.0, 120.0, 80.0),
        Band::Above,
    );
    sheet.rotate_canvas_object(source, 90.0).unwrap();
    let mut connector = CanvasConnector::new(
        Endpoint::Object(source),
        Endpoint::Point(Vec2::new(400.0, 220.0)),
    );
    connector.from_port = Some(ConnectorPort {
        position: Vec2::new(0.25, 0.75),
        offset: Vec2::new(5.0, -3.0),
        perimeter: false,
    });
    for route in [
        ConnectorRoute::Straight,
        ConnectorRoute::Elbow,
        ConnectorRoute::Curve,
    ] {
        connector.route = route;
        for waypoints in [vec![], vec![Vec2::new(250.0, 180.0)]] {
            connector.waypoints = waypoints.into();
            let layout = layout_sheet(&sheet, &ApproxMeasure, false);
            let path = connector_path(&sheet, &layout, &connector);
            assert!((path[0].x - 143.0).abs() < 0.01, "{route:?}: {path:?}");
            assert!((path[0].y - 115.0).abs() < 0.01, "{route:?}: {path:?}");
            assert_eq!(path.last(), Some(&Vec2::new(400.0, 220.0)));
        }
    }
    sheet
        .move_canvas_objects(&[source], Vec2::new(50.0, -20.0))
        .unwrap();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let path = connector_path(&sheet, &layout, &connector);
    assert!((path[0].x - 193.0).abs() < 0.01);
    assert!((path[0].y - 95.0).abs() < 0.01);
}

#[test]
fn fixed_connector_ports_project_onto_the_shape_perimeter() {
    let mut sheet = Sheet::new("Ports", "Root");
    let source = shape(
        &mut sheet,
        ShapeKind::Ellipse,
        rect(0.0, 0.0, 100.0, 100.0),
        Band::Above,
    );
    let mut connector = CanvasConnector::new(
        Endpoint::Object(source),
        Endpoint::Point(Vec2::new(200.0, 0.0)),
    );
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    connector.from_port = Some(ConnectorPort {
        position: Vec2::new(1.0, 1.0),
        offset: Vec2::ZERO,
        perimeter: true,
    });
    let projected = connector_path(&sheet, &layout, &connector)[0];
    assert!((projected.x - 85.355).abs() < 0.3, "{projected:?}");
    assert!((projected.y - 85.355).abs() < 0.3, "{projected:?}");
    connector.from_port.as_mut().unwrap().perimeter = false;
    assert_eq!(
        connector_path(&sheet, &layout, &connector)[0],
        Vec2::new(100.0, 100.0)
    );
}

#[test]
fn fixed_connector_ports_use_drawio_perimeters_before_rotation() {
    let mut sheet = Sheet::new("Ports", "Root");
    let rounded = shape(
        &mut sheet,
        ShapeKind::RoundedRectangle,
        rect(0.0, 0.0, 100.0, 60.0),
        Band::Above,
    );
    let ellipse = shape(
        &mut sheet,
        ShapeKind::Ellipse,
        rect(0.0, 0.0, 100.0, 60.0),
        Band::Above,
    );
    sheet.rotate_canvas_object(rounded, 90.0).unwrap();
    sheet.rotate_canvas_object(ellipse, 90.0).unwrap();
    let port = ConnectorPort {
        position: Vec2::new(1.0, 1.0),
        offset: Vec2::ZERO,
        perimeter: true,
    };
    let mut connector = CanvasConnector::new(
        Endpoint::Object(rounded),
        Endpoint::Point(Vec2::new(200.0, 0.0)),
    );
    connector.from_port = Some(port);
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let rounded_point = connector_path(&sheet, &layout, &connector)[0];
    assert!((rounded_point - Vec2::new(20.0, 80.0)).length() < 0.01);
    connector.from = Endpoint::Object(ellipse);
    let ellipse_point = connector_path(&sheet, &layout, &connector)[0];
    assert!(
        (ellipse_point - Vec2::new(28.787, 65.355)).length() < 0.3,
        "{ellipse_point:?}"
    );
}

#[test]
fn fixed_connector_ports_match_drawio_rectangle_ellipse_and_rhombus_perimeters() {
    let mut sheet = Sheet::new("Ports", "Root");
    let bounds = rect(0.0, 0.0, 100.0, 60.0);
    let rectangle = shape(&mut sheet, ShapeKind::Rectangle, bounds, Band::Above);
    let ellipse = shape(&mut sheet, ShapeKind::Ellipse, bounds, Band::Above);
    let diamond = shape(&mut sheet, ShapeKind::Diamond, bounds, Band::Above);
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);

    for (target, position, offset, expected) in [
        (
            rectangle,
            Vec2::new(0.5, 0.5),
            Vec2::ZERO,
            Vec2::new(100.0, 30.0),
        ),
        (
            rectangle,
            Vec2::new(0.25, 0.75),
            Vec2::new(5.0, -3.0),
            Vec2::new(0.0, 60.0),
        ),
        (
            ellipse,
            Vec2::new(1.0, 1.0),
            Vec2::ZERO,
            Vec2::new(85.35534, 51.2132),
        ),
        (
            ellipse,
            Vec2::new(0.5, 0.5),
            Vec2::new(0.9, -0.9),
            Vec2::new(50.9, 29.1),
        ),
        (
            ellipse,
            Vec2::new(0.508, 0.513_333_3),
            Vec2::ZERO,
            Vec2::new(50.8, 30.8),
        ),
        (
            ellipse,
            Vec2::new(0.513, 0.524),
            Vec2::ZERO,
            Vec2::new(75.724_785, 55.724_785),
        ),
        (
            diamond,
            Vec2::new(0.5, 0.5),
            Vec2::ZERO,
            Vec2::new(50.0, 60.0),
        ),
        (
            diamond,
            Vec2::new(0.25, 0.75),
            Vec2::new(5.0, -3.0),
            Vec2::new(50.0, 30.0),
        ),
        (
            diamond,
            Vec2::new(1.0, 1.0),
            Vec2::ZERO,
            Vec2::new(75.0, 45.0),
        ),
    ] {
        let mut connector = CanvasConnector::new(
            Endpoint::Object(target),
            Endpoint::Point(Vec2::new(200.0, 100.0)),
        );
        connector.from_port = Some(ConnectorPort {
            position,
            offset,
            perimeter: true,
        });
        let actual = connector_path(&sheet, &layout, &connector)[0];
        let object = sheet.canvas.object(target).unwrap();
        let CanvasObjectKind::Shape { shape, .. } = object.kind else {
            panic!("expected shape");
        };
        assert_eq!(
            fixed_connector_port_position(
                object.rect,
                object.rotation,
                Some((shape, object.rounding)),
                connector.from_port.unwrap(),
            ),
            actual
        );
        assert!(
            (actual - expected).length() < 0.001,
            "{target:?} {position:?} {offset:?}: {actual:?} != {expected:?}"
        );
    }
}

#[test]
fn connector_marker_geometry_and_hit_testing_follow_both_ends() {
    use crate::board::{ConnectorMarker, ConnectorMarkerKind};

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
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let (_, scene) = scene(&sheet);
    let item = scene.item(SceneId::Object(id)).unwrap();
    let SceneGeometry::Polyline {
        points,
        width,
        markers,
    } = &item.geometry
    else {
        panic!("expected connector");
    };
    assert_eq!(*width, 2.0);
    let start = markers[0].unwrap();
    let end = markers[1].unwrap();
    assert!(start.filled);
    assert!(!end.filled);
    assert!((start.points()[1].x - 2.236).abs() < 0.001);
    assert!((start.points()[0].x - 10.236).abs() < 0.001);
    assert!((end.points()[1].x - 97.764).abs() < 0.001);
    assert!((end.points()[0].x - 89.764).abs() < 0.001);
    assert!((end.points()[0].y.abs() - 4.0).abs() < 0.001);
    assert!(item.bounds.contains(end.points()[0]));
    assert_eq!(
        scene.hit_test_tolerance(end.points()[0], 0.0),
        Some(SceneHit::Object(id))
    );
    let line = connector_stroke_path(points, markers);
    assert!((line[0].x - 10.236).abs() < 0.001);
    assert!((line[1].x - 95.528).abs() < 0.001);
}

#[test]
fn connector_marker_geometry_handles_duplicate_and_short_segments() {
    use crate::board::ConnectorMarkerKind;

    let duplicate = [
        Vec2::ZERO,
        Vec2::ZERO,
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 0.0),
    ];
    let markers = [
        connector_marker_geometry(
            &duplicate,
            true,
            ConnectorMarkerKind::Open,
            2.0,
            DEFAULT_CONNECTOR_MARKER_SIZE,
            true,
        ),
        connector_marker_geometry(
            &duplicate,
            false,
            ConnectorMarkerKind::Block,
            2.0,
            DEFAULT_CONNECTOR_MARKER_SIZE,
            true,
        ),
    ];
    assert!(markers.iter().all(Option::is_some));
    let clipped = connector_stroke_path(&duplicate, &markers);
    assert!(clipped.len() >= 2);
    assert!(clipped.iter().all(|point| {
        point.x.is_finite() && point.y.is_finite() && (0.0..=1.0).contains(&point.x)
    }));
    assert!((clipped[0].x - clipped[clipped.len() - 1].x).abs() < 0.001);
    assert!(
        connector_marker_geometry(
            &[Vec2::ZERO, Vec2::ZERO],
            true,
            ConnectorMarkerKind::Open,
            2.0,
            DEFAULT_CONNECTOR_MARKER_SIZE,
            true,
        )
        .is_none()
    );

    let bent = [Vec2::ZERO, Vec2::new(2.0, 0.0), Vec2::new(2.0, 20.0)];
    let marker = connector_marker_geometry(
        &bent,
        true,
        ConnectorMarkerKind::Open,
        2.0,
        DEFAULT_CONNECTOR_MARKER_SIZE,
        true,
    );
    let clipped = connector_stroke_path(&bent, &[marker, None]);
    assert_eq!(clipped[0].x, 2.0);
    assert!((clipped[0].y - 2.472).abs() < 0.001);
    assert_eq!(clipped.last(), Some(&Vec2::new(2.0, 20.0)));
}

#[test]
fn connector_marker_geometry_matches_published_drawio_for_diagonal_edges() {
    use crate::board::ConnectorMarkerKind;

    // Recorded from app.diagrams.net's mxMarker with size 6 and stroke width 3.
    let points = [Vec2::new(140.0, 220.0), Vec2::new(200.0, 300.0)];
    for (kind, expected_end) in [
        (ConnectorMarkerKind::Open, Vec2::new(195.9752, 294.6336)),
        (ConnectorMarkerKind::Block, Vec2::new(192.5876, 290.1168)),
    ] {
        let marker = connector_marker_geometry(
            &points,
            false,
            kind,
            3.0,
            DEFAULT_CONNECTOR_MARKER_SIZE,
            true,
        )
        .unwrap();
        let expected = [
            Vec2::new(188.9876, 292.8168),
            Vec2::new(197.9876, 297.3168),
            Vec2::new(196.1876, 287.4168),
        ];
        for (actual, expected) in marker.points().iter().zip(expected) {
            assert!(
                (*actual - expected).length() < 0.001,
                "{actual:?} != {expected:?}"
            );
        }
        assert_eq!(marker.filled, kind == ConnectorMarkerKind::Block);
        let stroke = connector_stroke_path(&points, &[None, Some(marker)]);
        assert!((*stroke.last().unwrap() - expected_end).length() < 0.001);
    }
}

#[test]
fn classic_and_thin_marker_geometry_matches_drawio() {
    use crate::board::ConnectorMarkerKind;

    let points = [Vec2::ZERO, Vec2::new(100.0, 0.0)];
    for (kind, half_width, filled, end, notch) in [
        (ConnectorMarkerKind::Classic, 4.0, true, 91.764, true),
        (
            ConnectorMarkerKind::ClassicThin,
            8.0 / 3.0,
            true,
            91.764,
            true,
        ),
        (
            ConnectorMarkerKind::OpenThin,
            8.0 / 3.0,
            false,
            95.528,
            false,
        ),
        (
            ConnectorMarkerKind::BlockThin,
            8.0 / 3.0,
            true,
            89.764,
            false,
        ),
    ] {
        let marker = connector_marker_geometry(
            &points,
            false,
            kind,
            2.0,
            DEFAULT_CONNECTOR_MARKER_SIZE,
            true,
        )
        .unwrap();
        let outline = marker.points();
        assert_eq!(outline.len(), if notch { 4 } else { 3 });
        assert_eq!(marker.filled, filled);
        assert!((outline[0].x - 89.764).abs() < 0.001);
        assert!((outline[0].y - half_width).abs() < 0.001);
        assert!((outline[1].x - 97.764).abs() < 0.001);
        assert!((outline[2].y + half_width).abs() < 0.001);
        if notch {
            assert!((outline[3].x - 91.764).abs() < 0.001);
            assert!(!point_in_polygon(Vec2::new(90.0, 0.0), outline));
            assert!(point_in_polygon(Vec2::new(93.0, 0.0), outline));
        }
        let line = connector_stroke_path(&points, &[None, Some(marker)]);
        assert!((line.last().unwrap().x - end).abs() < 0.001);
        let start = connector_marker_geometry(
            &points,
            true,
            kind,
            2.0,
            DEFAULT_CONNECTOR_MARKER_SIZE,
            true,
        )
        .unwrap();
        assert!((start.points()[1].x - 2.236).abs() < 0.001);
        assert!(
            (connector_stroke_path(&points, &[Some(start), None])[0].x - (100.0 - end)).abs()
                < 0.001
        );
    }
}

#[test]
fn custom_marker_sizes_drive_both_end_geometry_and_hit_testing() {
    use crate::board::{ConnectorMarker, ConnectorMarkerKind};

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
    let (_, scene) = scene(&sheet);
    let item = scene.item(SceneId::Object(id)).unwrap();
    let SceneGeometry::Polyline {
        points, markers, ..
    } = &item.geometry
    else {
        panic!("expected connector");
    };
    let start = markers[0].unwrap();
    let end = markers[1].unwrap();
    assert!((start.points()[0].x - 4.236).abs() < 0.001);
    assert!((start.points()[0].y.abs() - 1.0).abs() < 0.001);
    assert!((end.points()[0].x - 83.264).abs() < 0.001);
    assert!((end.points()[0].y.abs() - 7.25).abs() < 0.001);
    assert!((end.points()[3].x - 86.889).abs() < 0.001);
    let line = connector_stroke_path(points, markers);
    assert!((line[0].x - 4.236).abs() < 0.001);
    assert!((line.last().unwrap().x - 86.889).abs() < 0.001);
    assert!(item.bounds.contains(end.points()[0]));
    assert_eq!(
        scene.hit_test_tolerance(end.points()[0], 0.0),
        Some(SceneHit::Object(id))
    );
}

#[test]
fn custom_marker_geometry_matches_published_drawio() {
    let points = [Vec2::ZERO, Vec2::new(100.0, 0.0)];
    for (kind, wing, trimmed_end, filled, classic) in [
        (ConnectorMarkerKind::Classic, 7.25, 86.889, true, true),
        (
            ConnectorMarkerKind::ClassicThin,
            14.5 / 3.0,
            86.889,
            true,
            true,
        ),
        (ConnectorMarkerKind::Open, 7.25, 95.528, false, false),
        (
            ConnectorMarkerKind::OpenThin,
            14.5 / 3.0,
            95.528,
            false,
            false,
        ),
        (ConnectorMarkerKind::Block, 7.25, 83.264, true, false),
        (
            ConnectorMarkerKind::BlockThin,
            14.5 / 3.0,
            83.264,
            true,
            false,
        ),
    ] {
        let marker = connector_marker_geometry(&points, false, kind, 2.0, 12.5, true).unwrap();
        let expected = [
            Vec2::new(83.264, wing),
            Vec2::new(97.764, 0.0),
            Vec2::new(83.264, -wing),
            Vec2::new(86.889, 0.0),
        ];
        assert_eq!(marker.points().len(), if classic { 4 } else { 3 });
        for (actual, expected) in marker.points().iter().zip(expected) {
            assert!(
                (*actual - expected).length() < 0.001,
                "{kind:?}: {actual:?}"
            );
        }
        assert_eq!(marker.filled, filled, "{kind:?}");
        assert!(
            (100.0 - marker.line_inset - trimmed_end).abs() < 0.001,
            "{kind:?}"
        );
    }
}

#[test]
fn marker_shapes_keep_scene_geometry_compact() {
    assert!(std::mem::size_of::<ConnectorMarkerGeometry>() <= 64);
    assert!(std::mem::size_of::<SceneGeometry>() <= 192);
}

#[test]
fn bar_marker_geometry_matches_published_drawio() {
    let horizontal = [Vec2::new(100.0, 100.0), Vec2::new(300.0, 100.0)];
    for (kind, at_start, size, expected) in [
        (
            ConnectorMarkerKind::BaseDash,
            true,
            12.5,
            [Vec2::new(100.0, 92.25), Vec2::new(100.0, 107.75)],
        ),
        (
            ConnectorMarkerKind::Dash,
            false,
            8.0,
            [Vec2::new(294.5, 105.5), Vec2::new(283.5, 94.5)],
        ),
        (
            ConnectorMarkerKind::BaseDash,
            false,
            0.0,
            [Vec2::new(300.0, 101.5), Vec2::new(300.0, 98.5)],
        ),
        (
            ConnectorMarkerKind::Dash,
            true,
            12.5,
            [Vec2::new(107.75, 92.25), Vec2::new(123.25, 107.75)],
        ),
    ] {
        for fill in [false, true] {
            let marker =
                connector_marker_geometry(&horizontal, at_start, kind, 2.0, size, fill).unwrap();
            assert_eq!(marker.points(), expected, "{kind:?}");
            assert!(!marker.closed && !marker.filled);
            assert_eq!(marker.line_inset, 0.0);
            assert_eq!(
                connector_stroke_path(&horizontal, &[Some(marker), Some(marker)]),
                horizontal
            );
        }
    }

    let diagonal = [Vec2::new(100.0, 100.0), Vec2::new(300.0, 200.0)];
    for (kind, at_start, size, expected) in [
        (
            ConnectorMarkerKind::BaseDash,
            true,
            12.5,
            [Vec2::new(103.4659, 93.0682), Vec2::new(96.5341, 106.9318)],
        ),
        (
            ConnectorMarkerKind::Dash,
            false,
            8.0,
            [Vec2::new(292.621, 202.4597), Vec2::new(287.7016, 187.7016)],
        ),
    ] {
        let marker = connector_marker_geometry(&diagonal, at_start, kind, 2.0, size, true).unwrap();
        for (actual, expected) in marker.points().iter().zip(expected) {
            assert!(
                (*actual - expected).length() < 0.001,
                "{kind:?}: {actual:?}"
            );
        }
    }
}

#[test]
fn bar_marker_hit_testing_tracks_only_the_visible_stroke() {
    use crate::board::ConnectorMarker;

    for (kind, on_marker, off_marker) in [
        (
            ConnectorMarkerKind::BaseDash,
            Vec2::new(100.0, 6.0),
            Vec2::new(98.0, 6.0),
        ),
        (
            ConnectorMarkerKind::Dash,
            Vec2::new(90.0, 3.6),
            Vec2::new(100.0, 6.0),
        ),
    ] {
        let mut sheet = Sheet::new("Bar marker", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.width = 0.1;
        connector.end_marker = Some(ConnectorMarker { kind });
        connector.end_marker_size = Some(12.5);
        let id = sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let (_, scene) = scene(&sheet);
        assert_eq!(
            scene.hit_test_tolerance(on_marker, 0.1),
            Some(SceneHit::Object(id)),
            "{kind:?}"
        );
        assert_eq!(scene.hit_test_tolerance(off_marker, 0.0), None, "{kind:?}");
    }
}

#[test]
fn half_circle_marker_geometry_matches_published_drawio() {
    let horizontal = [Vec2::new(100.0, 100.0), Vec2::new(300.0, 100.0)];
    let start = connector_marker_geometry(
        &horizontal,
        true,
        ConnectorMarkerKind::HalfCircle,
        2.0,
        12.5,
        true,
    )
    .unwrap();
    let end = connector_marker_geometry(
        &horizontal,
        false,
        ConnectorMarkerKind::HalfCircle,
        2.0,
        8.0,
        false,
    )
    .unwrap();
    assert_eq!(
        start.curve(),
        Some([
            Vec2::new(100.0, 84.5),
            Vec2::new(115.5, 84.5),
            Vec2::new(115.5, 100.0),
            Vec2::new(115.5, 115.5),
            Vec2::new(100.0, 115.5),
        ])
    );
    assert_eq!(
        end.curve(),
        Some([
            Vec2::new(300.0, 111.0),
            Vec2::new(289.0, 111.0),
            Vec2::new(289.0, 100.0),
            Vec2::new(289.0, 89.0),
            Vec2::new(300.0, 89.0),
        ])
    );
    assert!(!start.closed && !start.filled && !end.closed && !end.filled);
    assert_eq!(
        connector_stroke_path(&horizontal, &[Some(start), Some(end)]),
        vec![Vec2::new(115.5, 100.0), Vec2::new(289.0, 100.0)]
    );
    let zero = connector_marker_geometry(
        &horizontal,
        false,
        ConnectorMarkerKind::HalfCircle,
        2.0,
        0.0,
        true,
    )
    .unwrap();
    assert_eq!(zero.line_inset, 3.0);
    assert_eq!(zero.curve().unwrap()[2], Vec2::new(297.0, 100.0));

    let diagonal = [Vec2::new(100.0, 100.0), Vec2::new(300.0, 200.0)];
    let start = connector_marker_geometry(
        &diagonal,
        true,
        ConnectorMarkerKind::HalfCircle,
        3.0,
        12.5,
        true,
    )
    .unwrap();
    let end = connector_marker_geometry(
        &diagonal,
        false,
        ConnectorMarkerKind::HalfCircle,
        3.0,
        8.0,
        true,
    )
    .unwrap();
    for (actual, expected) in start.curve().unwrap().into_iter().zip([
        Vec2::new(107.38, 85.24),
        Vec2::new(122.14, 92.62),
        Vec2::new(114.76, 107.38),
        Vec2::new(107.38, 122.14),
        Vec2::new(92.62, 114.76),
    ]) {
        assert!((actual - expected).length() < 0.01, "{actual:?}");
    }
    for (actual, expected) in end.curve().unwrap().into_iter().zip([
        Vec2::new(294.63, 210.73),
        Vec2::new(283.90, 205.37),
        Vec2::new(289.27, 194.63),
        Vec2::new(294.63, 183.90),
        Vec2::new(305.37, 189.27),
    ]) {
        assert!((actual - expected).length() < 0.01, "{actual:?}");
    }
    let stroke = connector_stroke_path(&diagonal, &[Some(start), Some(end)]);
    assert!((stroke[0] - Vec2::new(114.76, 107.38)).length() < 0.01);
    assert!((stroke[1] - Vec2::new(289.27, 194.63)).length() < 0.01);
}

#[test]
fn half_circle_selection_path_is_bounded_and_tracks_the_curve() {
    let points = [Vec2::ZERO, Vec2::new(2000.0, 1000.0)];
    for width in [0.1, 2.0, 200.0] {
        for size in [0.0, 12.5, 999.0] {
            let marker = connector_marker_geometry(
                &points,
                false,
                ConnectorMarkerKind::HalfCircle,
                width,
                size,
                false,
            )
            .unwrap();
            let curve = marker.curve().unwrap();
            let (path, error) = marker.flattened_curve(width).unwrap();
            assert!(path.len() <= 1025);
            for span in [
                [curve[0], curve[1], curve[2]],
                [curve[2], curve[3], curve[4]],
            ] {
                for step in 0..=1000 {
                    let point = quadratic_point(span, f64::from(step) / 1000.0);
                    let point = Vec2::new(point[0] as f32, point[1] as f32);
                    assert!(
                        distance_to_polyline(point, &path) <= error + 0.001,
                        "width={width}, size={size}, error={error}, point={point:?}"
                    );
                }
            }
        }
    }
    let bounds = quadratic_bounds([
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
        Vec2::new(20.0, 0.0),
    ]);
    assert!(bounds.contains(Vec2::ZERO));
    assert!(bounds.contains(Vec2::new(20.0, 5.0)));
    assert!(bounds.size.x < 20.001 && bounds.size.y < 5.001);
}

#[test]
fn half_circle_flattening_remains_conservative_at_large_coordinates() {
    let points = [
        Vec2::new(999_900.0, 1_000_000.0),
        Vec2::new(1_000_000.0, 1_000_000.0),
    ];
    let marker = connector_marker_geometry(
        &points,
        false,
        ConnectorMarkerKind::HalfCircle,
        0.1,
        12.5,
        false,
    )
    .unwrap();
    let curve = marker.curve().unwrap();
    let (path, error) = marker.flattened_curve(0.1).unwrap();
    let distance = |point: [f64; 2], a: Vec2, b: Vec2| {
        let a = [f64::from(a.x), f64::from(a.y)];
        let delta = [f64::from(b.x) - a[0], f64::from(b.y) - a[1]];
        let length_squared = delta[0] * delta[0] + delta[1] * delta[1];
        let t = if length_squared == 0.0 {
            0.0
        } else {
            (((point[0] - a[0]) * delta[0] + (point[1] - a[1]) * delta[1]) / length_squared)
                .clamp(0.0, 1.0)
        };
        (point[0] - a[0] - t * delta[0]).hypot(point[1] - a[1] - t * delta[1])
    };
    for span in [
        [curve[0], curve[1], curve[2]],
        [curve[2], curve[3], curve[4]],
    ] {
        let bounds = quadratic_bounds(span);
        for step in 0..=1000 {
            let point = quadratic_point(span, f64::from(step) / 1000.0);
            let deviation = path
                .windows(2)
                .map(|pair| distance(point, pair[0], pair[1]))
                .fold(f64::INFINITY, f64::min);
            assert!(
                deviation <= f64::from(error) + 1e-8,
                "{deviation} > {error}"
            );
            assert!(point[0] >= f64::from(bounds.min.x));
            assert!(point[1] >= f64::from(bounds.min.y));
            assert!(point[0] <= f64::from(bounds.max().x));
            assert!(point[1] <= f64::from(bounds.max().y));
        }
    }
    let rotated_span = [
        Vec2::new(999_990.0, 999_990.0),
        Vec2::new(1_000_000.0, 1_000_000.0),
        Vec2::new(999_990.0, 1_000_010.0),
    ];
    let bounds = quadratic_bounds(rotated_span);
    for step in 0..=1000 {
        let point = quadratic_point(rotated_span, f64::from(step) / 1000.0);
        assert!(point[0] >= f64::from(bounds.min.x));
        assert!(point[1] >= f64::from(bounds.min.y));
        assert!(point[0] <= f64::from(bounds.max().x));
        assert!(point[1] <= f64::from(bounds.max().y));
    }
}

#[test]
fn half_circle_hit_distance_is_precise_at_large_coordinates() {
    let origin = Vec2::new(1_000_000.0, 1_000_000.0);
    let path = [origin, origin - Vec2::new(0.625, 0.625)];
    let point = origin - Vec2::new(0.625, 0.5625);
    assert!(point_near_curve_path(point, &path, 0.05));
    assert!(!point_near_curve_path(point, &path, 0.04));
    assert!(distance_to_polyline(point, &path) > 0.05);
}

#[test]
fn half_circle_hit_testing_respects_the_open_interior() {
    use crate::board::ConnectorMarker;

    let mut sheet = Sheet::new("Half-circle marker", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.width = 0.1;
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::HalfCircle,
    });
    connector.end_marker_size = Some(12.5);
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let (_, scene) = scene(&sheet);
    assert_eq!(scene.hit_test_tolerance(Vec2::new(100.0, 2.0), 0.0), None);
    assert_eq!(
        scene.hit_test_tolerance(Vec2::new(89.8, 10.2), 0.0),
        Some(SceneHit::Object(id))
    );
    let bounds = scene.item(SceneId::Object(id)).unwrap().bounds;
    assert!(bounds.contains(Vec2::new(100.0, 13.6)));
}

#[test]
fn oval_marker_geometry_matches_published_drawio() {
    let horizontal = [Vec2::new(100.0, 100.0), Vec2::new(300.0, 100.0)];
    let start = connector_marker_geometry(
        &horizontal,
        true,
        ConnectorMarkerKind::Oval,
        2.0,
        12.5,
        false,
    )
    .unwrap();
    let end = connector_marker_geometry(
        &horizontal,
        false,
        ConnectorMarkerKind::Oval,
        2.0,
        8.0,
        true,
    )
    .unwrap();
    assert_eq!(start.circle(), Some((horizontal[0], 6.25)));
    assert_eq!(end.circle(), Some((horizontal[1], 4.0)));
    assert!(start.points().is_empty() && end.points().is_empty());
    assert!(!start.filled && end.filled);
    assert_eq!(
        connector_stroke_path(&horizontal, &[Some(start), Some(end)]),
        vec![Vec2::new(106.25, 100.0), Vec2::new(296.0, 100.0)]
    );

    let diagonal = [Vec2::new(140.0, 220.0), Vec2::new(200.0, 300.0)];
    let marker =
        connector_marker_geometry(&diagonal, false, ConnectorMarkerKind::Oval, 8.0, 8.0, true)
            .unwrap();
    assert_eq!(marker.circle(), Some((diagonal[1], 4.0)));
    let line = connector_stroke_path(&diagonal, &[None, Some(marker)]);
    assert!((*line.last().unwrap() - Vec2::new(197.6, 296.8)).length() < 0.001);

    let zero = connector_marker_geometry(
        &horizontal,
        false,
        ConnectorMarkerKind::Oval,
        2.0,
        0.0,
        true,
    )
    .unwrap();
    assert_eq!(zero.circle(), Some((horizontal[1], 0.0)));
    assert_eq!(
        connector_stroke_path(&horizontal, &[None, Some(zero)]),
        horizontal
    );
}

#[test]
fn oval_marker_hit_testing_respects_hollow_interiors_and_stroke_bounds() {
    use crate::board::ConnectorMarker;

    for filled in [false, true] {
        let mut sheet = Sheet::new("Oval marker", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.width = 0.1;
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Oval,
        });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = !filled;
        let id = sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let (_, scene) = scene(&sheet);
        let item = scene.item(SceneId::Object(id)).unwrap();
        assert!(item.bounds.contains(Vec2::new(100.0, 6.29)));
        assert_eq!(
            scene.hit_test_tolerance(Vec2::new(100.0, 1.0), 0.0),
            filled.then_some(SceneHit::Object(id))
        );
        assert_eq!(
            scene.hit_test_tolerance(Vec2::new(100.0, 6.25), 0.0),
            Some(SceneHit::Object(id))
        );
    }
}

#[test]
fn diamond_marker_geometry_matches_published_drawio() {
    let horizontal = [Vec2::new(0.0, 50.0), Vec2::new(100.0, 50.0)];
    for (kind, expected, trimmed) in [
        (
            ConnectorMarkerKind::Diamond,
            [
                Vec2::new(92.5858, 56.0),
                Vec2::new(98.5858, 50.0),
                Vec2::new(92.5858, 44.0),
                Vec2::new(86.5858, 50.0),
            ],
            86.5858,
        ),
        (
            ConnectorMarkerKind::DiamondThin,
            [
                Vec2::new(92.0276, 53.52941),
                Vec2::new(98.0276, 50.0),
                Vec2::new(92.0276, 46.47059),
                Vec2::new(86.0276, 50.0),
            ],
            86.0276,
        ),
    ] {
        for fill in [false, true] {
            let marker =
                connector_marker_geometry(&horizontal, false, kind, 2.0, 10.0, fill).unwrap();
            assert!(marker.closed);
            assert_eq!(marker.filled, fill);
            for (actual, expected) in marker.points().iter().zip(expected) {
                assert!(
                    (*actual - expected).length() < 0.001,
                    "{kind:?}: {actual:?}"
                );
            }
            let line = connector_stroke_path(&horizontal, &[None, Some(marker)]);
            assert!((line.last().unwrap().x - trimmed).abs() < 0.001);
            let start =
                connector_marker_geometry(&horizontal, true, kind, 2.0, 10.0, fill).unwrap();
            for (actual, expected) in start.points().iter().zip(expected) {
                let reflected = Vec2::new(100.0 - expected.x, 100.0 - expected.y);
                assert!(
                    (*actual - reflected).length() < 0.001,
                    "{kind:?}: {actual:?}"
                );
            }
            let line = connector_stroke_path(&horizontal, &[Some(start), None]);
            assert!((line[0].x - (100.0 - trimmed)).abs() < 0.001);
        }
    }

    // Recorded from app.diagrams.net at a 3:4 direction, size 6 and width 2.
    let diagonal = [Vec2::new(140.0, 220.0), Vec2::new(200.0, 300.0)];
    for (kind, expected) in [
        (
            ConnectorMarkerKind::Diamond,
            [
                Vec2::new(193.55148, 298.06864),
                Vec2::new(199.15148, 298.86864),
                Vec2::new(199.95148, 293.26865),
                Vec2::new(194.35149, 292.46863),
            ],
        ),
        (
            ConnectorMarkerKind::DiamondThin,
            [
                Vec2::new(194.53421, 296.63385),
                Vec2::new(198.81656, 298.422_1),
                Vec2::new(198.2989, 293.8103),
                Vec2::new(194.01656, 292.0221),
            ],
        ),
    ] {
        let marker = connector_marker_geometry(&diagonal, false, kind, 2.0, 6.0, true).unwrap();
        for (actual, expected) in marker.points().iter().zip(expected) {
            assert!(
                (*actual - expected).length() < 0.001,
                "{kind:?}: {actual:?}"
            );
        }
        let line = connector_stroke_path(&diagonal, &[None, Some(marker)]);
        assert!((*line.last().unwrap() - expected[3]).length() < 0.001);
    }
}

#[test]
fn hollow_marker_geometry_is_closed_but_does_not_hit_the_empty_interior() {
    use crate::board::{ConnectorMarker, ConnectorMarkerKind};

    for kind in [
        ConnectorMarkerKind::Classic,
        ConnectorMarkerKind::ClassicThin,
        ConnectorMarkerKind::Block,
        ConnectorMarkerKind::BlockThin,
        ConnectorMarkerKind::Diamond,
        ConnectorMarkerKind::DiamondThin,
    ] {
        let mut sheet = Sheet::new("Hollow markers", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.width = 0.1;
        connector.end_marker = Some(ConnectorMarker { kind });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        let id = sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let (_, scene) = scene(&sheet);
        let SceneGeometry::Polyline { markers, .. } =
            &scene.item(SceneId::Object(id)).unwrap().geometry
        else {
            panic!("expected connector");
        };
        let marker = markers[1].unwrap();
        assert!(marker.closed, "{kind:?}");
        assert!(!marker.filled, "{kind:?}");
        assert_eq!(
            scene.hit_test_tolerance(Vec2::new(95.0, 0.0), 0.0),
            None,
            "{kind:?}"
        );
        let closing = (marker.points()[0] + marker.points()[marker.points().len() - 1]) * 0.5;
        assert_eq!(
            scene.hit_test_tolerance(closing, 0.0),
            Some(SceneHit::Object(id)),
            "{kind:?}"
        );
        let filled = connector_marker_geometry(
            &[Vec2::ZERO, Vec2::new(100.0, 0.0)],
            false,
            kind,
            0.1,
            12.5,
            true,
        )
        .unwrap();
        assert_eq!(marker.line_inset, filled.line_inset);
    }
    for kind in [ConnectorMarkerKind::Open, ConnectorMarkerKind::OpenThin] {
        let points = [Vec2::ZERO, Vec2::new(100.0, 0.0)];
        for fill in [false, true] {
            let marker = connector_marker_geometry(&points, false, kind, 2.0, 12.5, fill).unwrap();
            assert!(!marker.closed && !marker.filled, "{kind:?}, {fill}");
        }
    }
}

#[test]
fn classic_marker_hit_testing_respects_the_notch() {
    use crate::board::{ConnectorMarker, ConnectorMarkerKind};

    let mut sheet = Sheet::new("Markers", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.width = 0.1;
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Classic,
    });
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let (_, scene) = scene(&sheet);
    assert_eq!(
        scene.hit_test_tolerance(Vec2::new(93.8, 3.0), 0.0),
        Some(SceneHit::Object(id))
    );
    assert_eq!(scene.hit_test_tolerance(Vec2::new(94.2, 0.9), 0.0), None);
}

#[test]
fn bands_paint_in_order() {
    let mut s = Sheet::new("Bands", "Root");
    let f = s.add_floating("F", Vec2::new(0.0, 400.0));
    let zone = s
        .create_zone("Z", vec![f], rect(-200.0, 300.0, 400.0, 200.0))
        .unwrap();
    // Insert Above before Below to prove ordering is by band, not Vec order.
    let above = shape(
        &mut s,
        ShapeKind::Rectangle,
        rect(-20.0, -20.0, 40.0, 40.0),
        Band::Above,
    );
    let below = shape(
        &mut s,
        ShapeKind::Ellipse,
        rect(-20.0, -20.0, 40.0, 40.0),
        Band::Below,
    );
    s.board.grid = GridKind::Lines;
    let (_, sc) = scene(&s);
    let pos = |id: SceneId| sc.items.iter().position(|i| i.id == id).unwrap();
    let bands: Vec<_> = sc.items.iter().map(|i| i.band).collect();
    assert!(bands.windows(2).all(|w| w[0] <= w[1]), "{bands:?}");
    assert!(pos(SceneId::Zone(zone)) < pos(SceneId::Object(below)));
    assert!(pos(SceneId::Object(below)) < pos(SceneId::Topic(s.root)));
    assert!(pos(SceneId::Topic(s.root)) < pos(SceneId::Object(above)));
    assert_eq!(sc.grid, Some((GridKind::Lines, s.board.grid_size)));

    // The Above object covers the root; the Below object is covered by it.
    assert_eq!(sc.hit_test(Vec2::ZERO), Some(SceneHit::Object(above)));
    assert_eq!(
        sc.hit_test_tolerance_excluding(Vec2::ZERO, HIT_TOLERANCE, Some(SceneId::Object(above))),
        Some(SceneHit::Topic(s.root))
    );
    s.set_canvas_band(&[above], Band::Below).unwrap();
    let (_, sc) = scene(&s);
    assert_eq!(sc.hit_test(Vec2::ZERO), Some(SceneHit::Topic(s.root)));
    // Zones never hit; an empty spot inside the frame is a miss.
    assert_eq!(sc.hit_test(Vec2::new(-190.0, 310.0)), None);
}

#[test]
fn clipping_frames_constrain_hits_selection_and_visible_bounds() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet.add_floating("Card", Vec2::new(50.0, 50.0));
    sheet
        .set_topic_card(
            root,
            Some(Card::new(CardKind::Sticky, Vec2::new(140.0, 80.0))),
        )
        .unwrap();
    let zone = sheet
        .create_zone("Frame", vec![root], rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|frame| frame.id == zone)
        .unwrap()
        .clip = true;
    let object = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(-30.0, 20.0, 80.0, 40.0),
        Band::Above,
    );
    let (_, scene) = scene(&sheet);
    let clip = rect(0.0, 0.0, 100.0, 100.0);
    assert_eq!(scene.item(SceneId::Topic(root)).unwrap().clip, Some(clip));
    assert_eq!(
        scene.item(SceneId::Object(object)).unwrap().clip,
        Some(clip)
    );
    assert_eq!(
        scene.hit_test(Vec2::new(20.0, 40.0)),
        Some(SceneHit::Object(object))
    );
    assert_eq!(scene.hit_test(Vec2::new(-10.0, 40.0)), None);
    assert!(
        !scene
            .objects_in(rect(-25.0, 25.0, 10.0, 10.0))
            .contains(&SceneHit::Object(object))
    );
    assert_eq!(scene.bounds(), Some(clip));
}

#[test]
fn summary_topics_inherit_their_owner_frame_clip() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet.add_floating("Root", Vec2::new(50.0, 50.0));
    let first = sheet.add_child(root, None, "First");
    let second = sheet.add_child(root, None, "Second");
    let summary = sheet.add_summary(&[first, second], "Summary").unwrap();
    let nested = sheet.add_child(summary, None, "Detail");
    let clip = rect(0.0, 0.0, 100.0, 100.0);
    let zone = sheet.create_zone("Frame", vec![root], clip).unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|frame| frame.id == zone)
        .unwrap()
        .clip = true;
    assert_eq!(topic_clip_rect(&sheet, summary), Some(clip));
    assert_eq!(topic_clip_rect(&sheet, nested), Some(clip));
}

#[test]
fn attached_connectors_use_resolved_path_for_frame_clipping() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let clip = rect(900.0, 900.0, 200.0, 200.0);
    let frame = sheet.create_board_frame("Frame", clip).unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|zone| zone.id == frame)
        .unwrap()
        .clip = true;
    let first = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(940.0, 940.0, 20.0, 20.0),
        Band::Above,
    );
    let second = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(1140.0, 940.0, 20.0, 20.0),
        Band::Above,
    );
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(first),
            Endpoint::Object(second),
        )))
        .unwrap();
    let (_, clipped) = scene(&sheet);
    assert_eq!(
        clipped.item(SceneId::Object(connector)).unwrap().clip,
        Some(clip)
    );
    assert_eq!(clipped.hit_test(Vec2::new(1120.0, 950.0)), None);

    sheet
        .move_canvas_objects(&[first, second], Vec2::new(500.0, 0.0))
        .unwrap();
    let (_, moved) = scene(&sheet);
    assert_eq!(moved.item(SceneId::Object(connector)).unwrap().clip, None);
}

#[test]
fn rotated_shape_hit_test() {
    let mut s = Sheet::new("Rot", "Root");
    let id = shape(
        &mut s,
        ShapeKind::Rectangle,
        rect(1000.0, 0.0, 200.0, 20.0),
        Band::Above,
    );
    s.rotate_canvas_object(id, 90.0).unwrap();
    let (_, sc) = scene(&s);
    // Center (1100,10); rotated it spans x 1090..1110, y -90..110.
    assert_eq!(
        sc.hit_test(Vec2::new(1100.0, 100.0)),
        Some(SceneHit::Object(id))
    );
    assert_eq!(sc.hit_test(Vec2::new(1005.0, 10.0)), None);
    assert_eq!(sc.hit_test(Vec2::new(1190.0, 10.0)), None);
    let b = sc.item(SceneId::Object(id)).unwrap().bounds;
    assert!(
        (b.size.x - 20.0).abs() < 0.5 && (b.size.y - 200.0).abs() < 0.5,
        "{b:?}"
    );

    // Non-rectangular outline: a diamond's corners are outside.
    let d = shape(
        &mut s,
        ShapeKind::Diamond,
        rect(2000.0, 0.0, 100.0, 100.0),
        Band::Above,
    );
    let (_, sc) = scene(&s);
    assert_eq!(
        sc.hit_test(Vec2::new(2050.0, 50.0)),
        Some(SceneHit::Object(d))
    );
    assert_eq!(sc.hit_test(Vec2::new(2005.0, 5.0)), None);
}

#[test]
fn ink_hit_respects_width_and_rotation() {
    let mut s = Sheet::new("Ink", "Root");
    let id = s
        .add_canvas_object(CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(0, 0, 0),
            10.0,
            vec![Vec2::new(1000.0, 0.0), Vec2::new(1100.0, 0.0)],
        )))
        .unwrap();
    let (_, sc) = scene(&s);
    assert_eq!(
        sc.hit_test(Vec2::new(1050.0, 7.0)),
        Some(SceneHit::Object(id))
    );
    assert_eq!(sc.hit_test(Vec2::new(1050.0, 9.0)), None);
    s.rotate_canvas_object(id, 90.0).unwrap();
    let (_, sc) = scene(&s);
    assert_eq!(
        sc.hit_test(Vec2::new(1050.0, 40.0)),
        Some(SceneHit::Object(id))
    );
    assert_eq!(sc.hit_test(Vec2::new(1090.0, 0.0)), None);
}

#[test]
fn waypoint_paths_clip_to_shapes_and_participate_in_hit_testing() {
    let mut sheet = Sheet::new("Waypoints", "Root");
    let from = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(1000.0, 1000.0, 40.0, 40.0),
        Band::Above,
    );
    let to = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(1200.0, 1000.0, 40.0, 40.0),
        Band::Above,
    );
    let mut connector = CanvasConnector::new(Endpoint::Object(from), Endpoint::Object(to));
    connector.waypoints = vec![Vec2::new(1020.0, 1100.0), Vec2::new(1220.0, 1100.0)].into();
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector.clone()))
        .unwrap();
    let (layout, scene) = scene(&sheet);
    let path = connector_path(&sheet, &layout, &connector);
    assert_eq!(
        path,
        vec![
            Vec2::new(1020.0, 1040.0),
            Vec2::new(1020.0, 1100.0),
            Vec2::new(1220.0, 1100.0),
            Vec2::new(1220.0, 1040.0),
        ]
    );
    assert_eq!(
        scene.hit_test(Vec2::new(1120.0, 1102.0)),
        Some(SceneHit::Object(id))
    );
    assert_eq!(scene.hit_test(Vec2::new(1120.0, 1050.0)), None);
    let SceneGeometry::Polyline { points, .. } = &scene.item(SceneId::Object(id)).unwrap().geometry
    else {
        panic!("expected connector path");
    };
    assert_eq!(points, &path);
}

#[test]
fn waypoint_routes_preserve_controls_and_keep_elbows_orthogonal() {
    let sheet = Sheet::new("Waypoints", "Root");
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let start = Vec2::new(1000.0, 1200.0);
    let control = Vec2::new(1050.0, 1260.0);
    let end = Vec2::new(1100.0, 1300.0);
    let mut connector = CanvasConnector::new(Endpoint::Point(start), Endpoint::Point(end));
    connector.waypoints = vec![control].into();
    assert_eq!(
        connector_path(&sheet, &layout, &connector),
        vec![start, control, end]
    );

    connector.route = ConnectorRoute::Elbow;
    let elbow = connector_path(&sheet, &layout, &connector);
    assert!(elbow.contains(&control));
    assert!(
        elbow
            .windows(2)
            .all(|pair| { pair[0].x == pair[1].x || pair[0].y == pair[1].y })
    );

    connector.route = ConnectorRoute::Curve;
    connector.waypoints = vec![Vec2::new(1050.0, 1300.0)].into();
    let curve = connector_path(&sheet, &layout, &connector);
    assert_eq!(curve.first(), Some(&start));
    assert_eq!(curve.last(), Some(&end));
    assert_eq!(curve[4], Vec2::new(1050.0, 1275.0));

    connector.waypoints = vec![start, end].into();
    let curve = connector_path(&sheet, &layout, &connector);
    assert_eq!(curve.first(), Some(&start));
    assert_eq!(curve.last(), Some(&end));
    assert!(
        curve
            .iter()
            .all(|point| ((point.x - start.x) - (point.y - start.y)).abs() < 0.001)
    );

    connector.waypoints = vec![
        Vec2::new(1020.0, 1280.0),
        Vec2::new(1020.0, 1280.0),
        Vec2::new(1080.0, 1280.0),
    ]
    .into();
    let curve = connector_path(&sheet, &layout, &connector);
    assert!(
        curve.contains(&Vec2::new(1020.0, 1280.0)),
        "repeated controls are significant"
    );
}

#[test]
fn waypoint_insertion_handles_follow_rendered_segments_and_indices() {
    let sheet = Sheet::new("Waypoints", "Root");
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let start = Vec2::new(0.0, 0.0);
    let end = Vec2::new(200.0, 150.0);
    let mut connector = CanvasConnector::new(Endpoint::Point(start), Endpoint::Point(end));
    connector.route = ConnectorRoute::Elbow;
    connector.waypoints = vec![Vec2::new(100.0, 100.0)].into();
    let path = connector_path(&sheet, &layout, &connector);
    let handles = connector_insert_handles(&sheet, &layout, &connector);
    assert_eq!(
        handles
            .iter()
            .map(|handle| handle.insert_at)
            .collect::<Vec<_>>(),
        [0, 0, 1, 1]
    );
    assert_eq!(handles.len(), path.len() - 1);
    for (handle, segment) in handles.iter().zip(path.windows(2)) {
        assert_eq!(handle.point, (segment[0] + segment[1]) * 0.5);
    }

    connector.route = ConnectorRoute::Straight;
    connector.waypoints = vec![
        start,
        Vec2::new(60.0, 0.0),
        Vec2::new(60.0, 0.0),
        Vec2::new(140.0, 0.0),
    ]
    .into();
    assert_eq!(
        connector_insert_handles(&sheet, &layout, &connector)
            .iter()
            .map(|handle| handle.insert_at)
            .collect::<Vec<_>>(),
        [1, 3, 4]
    );
}

#[test]
fn waypoint_insertion_indices_follow_revisited_clipped_endpoint() {
    let mut sheet = Sheet::new("Waypoints", "Root");
    let from = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(1000.0, 1000.0, 40.0, 40.0),
        Band::Above,
    );
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let clipped = Vec2::new(1020.0, 1040.0);
    let mut connector = CanvasConnector::new(
        Endpoint::Object(from),
        Endpoint::Point(Vec2::new(1300.0, 1100.0)),
    );
    connector.waypoints = vec![
        clipped,
        Vec2::new(1020.0, 1100.0),
        clipped,
        Vec2::new(1120.0, 1040.0),
    ]
    .into();
    for (route, indices) in [
        (ConnectorRoute::Straight, vec![1, 2, 3, 4]),
        (ConnectorRoute::Elbow, vec![1, 2, 3, 4, 4]),
    ] {
        connector.route = route;
        let path = connector_path(&sheet, &layout, &connector);
        assert_eq!(path[0], clipped, "{route:?}");
        assert_eq!(path[2], clipped, "{route:?}");
        let handles = connector_insert_handles(&sheet, &layout, &connector);
        assert_eq!(
            handles
                .iter()
                .map(|handle| handle.insert_at)
                .collect::<Vec<_>>(),
            indices,
            "{route:?}"
        );
        for (handle, segment) in handles.iter().zip(path.windows(2)) {
            assert_eq!(handle.point, (segment[0] + segment[1]) * 0.5);
        }
    }
}

#[test]
fn curved_waypoint_insertion_handles_lie_on_the_sampled_curve() {
    let sheet = Sheet::new("Waypoints", "Root");
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(0.0, 0.0)),
        Endpoint::Point(Vec2::new(200.0, 150.0)),
    );
    connector.route = ConnectorRoute::Curve;
    for waypoints in [
        vec![],
        vec![Vec2::new(50.0, 120.0), Vec2::new(150.0, 30.0)],
        vec![Vec2::new(50.0, 120.0), Vec2::new(50.0, 120.0)],
    ] {
        connector.waypoints = waypoints.into();
        let path = connector_path(&sheet, &layout, &connector);
        let handles = connector_insert_handles(&sheet, &layout, &connector);
        assert_eq!(
            handles.len(),
            if connector.waypoints.is_empty() {
                1
            } else {
                connector.waypoints.len() * 2
            }
        );
        assert!(
            handles
                .iter()
                .all(|handle| distance_to_polyline(handle.point, &path) < 0.001)
        );
        if connector.waypoints.len() == 2 {
            assert_eq!(
                handles
                    .iter()
                    .map(|handle| handle.insert_at)
                    .collect::<Vec<_>>(),
                [0, 1, 1, 2]
            );
        }
    }
}

#[test]
fn connectors_resolve_endpoints_and_hit() {
    let mut s = Sheet::new("Conn", "Root");
    let f = s.add_floating("F", Vec2::new(600.0, 0.0));
    let box_id = shape(
        &mut s,
        ShapeKind::Rectangle,
        rect(280.0, 280.0, 40.0, 40.0),
        Band::Above,
    );
    let straight = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(s.root),
            Endpoint::Topic(f),
        )))
        .unwrap();
    let elbow = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector {
            route: ConnectorRoute::Elbow,
            ..CanvasConnector::new(
                Endpoint::Point(Vec2::new(0.0, 600.0)),
                Endpoint::Object(box_id),
            )
        }))
        .unwrap();
    let curve = CanvasConnector {
        route: ConnectorRoute::Curve,
        ..CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 100.0)),
        )
    };
    let (layout, sc) = scene(&s);
    let conn = |id| match &s.canvas.object(id).unwrap().kind {
        CanvasObjectKind::Connector(c) => c.clone(),
        _ => unreachable!(),
    };
    let path = connector_path(&s, &layout, &conn(straight));
    assert_eq!(path.len(), 2);
    // Clipped to the topic boundaries rather than their centers.
    let root = layout.rect(s.root).unwrap();
    let fr = layout.rect(f).unwrap();
    assert!((path[0].x - root.max().x).abs() < 1.0, "{path:?} {root:?}");
    assert!((path[1].x - fr.min.x).abs() < 1.0, "{path:?} {fr:?}");
    let mid = (path[0] + path[1]) * 0.5;
    assert_eq!(
        sc.hit_test(mid + Vec2::new(0.0, 2.0)),
        Some(SceneHit::Object(straight))
    );
    assert_eq!(sc.hit_test(mid + Vec2::new(0.0, 20.0)), None);

    let path = connector_path(&s, &layout, &conn(elbow));
    assert_eq!(path.len(), 4);
    assert_eq!(path[0], Vec2::new(0.0, 600.0));
    assert!(rect(280.0, 280.0, 40.0, 40.0).expand(0.5).contains(path[3]));
    assert!(sc.item(SceneId::Object(elbow)).is_some());
    assert_eq!(connector_path(&s, &layout, &curve).len(), 17);

    // An unresolvable endpoint yields no path and no scene item, never a panic.
    let dangling = CanvasConnector::new(
        Endpoint::Object(CanvasObjectId(99_999)),
        Endpoint::Point(Vec2::ZERO),
    );
    assert!(connector_path(&s, &layout, &dangling).is_empty());
    let mut broken = s.clone();
    if let CanvasObjectKind::Connector(c) = &mut broken.canvas.object_mut(straight).unwrap().kind {
        c.to = Endpoint::Topic(TopicId(99_999));
    }
    let layout = layout_sheet(&broken, &ApproxMeasure, false);
    assert!(
        build_scene(&broken, &layout)
            .item(SceneId::Object(straight))
            .is_none()
    );
}

#[test]
fn connectors_end_on_shape_perimeters() {
    let mut s = Sheet::new("Perimeters", "");
    s.root_mode = SheetRootMode::Hidden;
    let corner = Endpoint::Point(Vec2::new(300.0, 300.0));
    let mut end_on = |kind, rotation: f32, toward: Endpoint| {
        let id = s
            .add_canvas_object(CanvasObject {
                rotation,
                ..CanvasObject::shape(kind, rect(0.0, 0.0, 100.0, 100.0))
            })
            .unwrap();
        let layout = layout_sheet(&s, &ApproxMeasure, false);
        let start = connector_path(
            &s,
            &layout,
            &CanvasConnector::new(Endpoint::Object(id), toward),
        )[0];
        s.remove_canvas_objects(&[id]).unwrap();
        start
    };
    let near = |a: Vec2, b: Vec2| (a - b).length() < 1.0;

    // Rectangles keep their box corner; a diamond and an ellipse end on their edge.
    assert!(near(
        end_on(ShapeKind::Rectangle, 0.0, corner),
        Vec2::new(100.0, 100.0)
    ));
    assert!(near(
        end_on(ShapeKind::Diamond, 0.0, corner),
        Vec2::new(75.0, 75.0)
    ));
    let diagonal = 50.0 + 50.0 / 2f32.sqrt();
    let ellipse = end_on(ShapeKind::Ellipse, 0.0, corner);
    assert!(near(ellipse, Vec2::new(diagonal, diagonal)), "{ellipse:?}");

    // Rotation turns the outline: a diamond rotated 45 degrees is an upright square.
    let rotated = end_on(
        ShapeKind::Diamond,
        45.0,
        Endpoint::Point(Vec2::new(300.0, 50.0)),
    );
    assert!(near(rotated, Vec2::new(diagonal, 50.0)), "{rotated:?}");

    // Every shape's end lies on its outline, never inside or on the bare box.
    for kind in ShapeKind::ALL {
        let end = end_on(kind, 30.0, corner);
        let outline = shape_geometry(kind, rect(0.0, 0.0, 100.0, 100.0), 30.0).outline;
        assert!(
            distance_to_outline(end, &outline) < 0.5,
            "{kind:?} ends at {end:?}"
        );
    }
}

#[test]
fn polygon_clip_takes_the_outermost_crossing() {
    // A notch cut from the top: the ray to the right leaves at x 60, re-enters at 80
    // and exits at 100.
    let notched = [
        Vec2::new(0.0, 0.0),
        Vec2::new(60.0, 0.0),
        Vec2::new(60.0, 55.0),
        Vec2::new(80.0, 55.0),
        Vec2::new(80.0, 0.0),
        Vec2::new(100.0, 0.0),
        Vec2::new(100.0, 100.0),
        Vec2::new(0.0, 100.0),
    ];
    let center = Vec2::new(50.0, 50.0);
    assert_eq!(
        clip_to_polygon(&notched, center, Vec2::new(200.0, 50.0)),
        Some(Vec2::new(100.0, 50.0))
    );
    assert_eq!(
        clip_to_polygon(&notched, center, Vec2::new(50.0, -200.0)),
        Some(Vec2::new(50.0, 0.0))
    );
    assert_eq!(clip_to_polygon(&notched, center, center), None);
}

#[test]
fn adjustable_corners_drive_scene_hits_and_connector_clipping() {
    use crate::board::{DrawioRectangleStencil, ShapeRounding};

    let mut sheet = Sheet::new("Rounded", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let rounding = ShapeRounding {
        radius: 40.0,
        stencil: DrawioRectangleStencil::Misc,
    };
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 100.0, 100.0));
    object.rounding = Some(rounding);
    let id = sheet.add_canvas_object(object).unwrap();
    let (layout, initial_scene) = scene(&sheet);
    assert_eq!(initial_scene.hit_test(Vec2::new(3.0, 3.0)), None);
    assert_eq!(
        initial_scene.hit_test(Vec2::new(50.0, 50.0)),
        Some(SceneHit::Object(id))
    );
    let edge = CanvasConnector::new(
        Endpoint::Object(id),
        Endpoint::Point(Vec2::new(200.0, 200.0)),
    );
    let end = connector_path(&sheet, &layout, &edge)[0];
    let expected = 60.0 + 40.0 / 2.0_f32.sqrt();
    assert!((end - Vec2::new(expected, expected)).length() < 0.01);

    sheet
        .resize_canvas_object(id, rect(0.0, 0.0, 200.0, 100.0))
        .unwrap();
    sheet.rotate_canvas_object(id, 30.0).unwrap();
    assert_eq!(sheet.canvas.object(id).unwrap().rounding, Some(rounding));
    let (layout, scene) = scene(&sheet);
    let item = scene
        .items
        .iter()
        .find(|item| item.id == SceneId::Object(id))
        .unwrap();
    let SceneGeometry::Polygon { outline, .. } = &item.geometry else {
        panic!("expected rounded shape geometry");
    };
    let endpoint = connector_path(&sheet, &layout, &edge)[0];
    assert!(distance_to_outline(endpoint, outline) < 0.01);
    assert_eq!(
        outline,
        &shape_geometry_with_rounding(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 200.0, 100.0),
            30.0,
            Some(rounding),
        )
        .outline
    );
}

#[test]
fn double_ellipse_scene_uses_effective_width_and_outer_perimeter() {
    let mut sheet = Sheet::new("Double ellipse", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let frame = rect(0.0, 0.0, 100.0, 60.0);
    let mut object = CanvasObject::shape(ShapeKind::DoubleEllipse, frame);
    object.style.stroke_width = 9.0;
    let id = sheet.add_canvas_object(object).unwrap();
    let (layout, initial_scene) = scene(&sheet);
    let item = initial_scene.item(SceneId::Object(id)).unwrap();
    let SceneGeometry::Polygon { outline, details } = &item.geometry else {
        panic!("expected double-ellipse geometry");
    };
    assert_eq!(
        details,
        &crate::board::shape_geometry_with_style(
            ShapeKind::DoubleEllipse,
            frame,
            0.0,
            None,
            2.0,
            None,
        )
        .details
    );
    assert_eq!(
        initial_scene.hit_test(frame.center()),
        Some(SceneHit::Object(id))
    );
    let edge = CanvasConnector::new(
        Endpoint::Object(id),
        Endpoint::Point(Vec2::new(200.0, 30.0)),
    );
    let endpoint = connector_path(&sheet, &layout, &edge)[0];
    assert!(distance_to_outline(endpoint, outline) < 0.01);
    assert!((endpoint.x - 100.0).abs() < 0.01);

    sheet.canvas.object_mut(id).unwrap().style.stroke = Some(Color::BLACK);
    let (_, scene) = scene(&sheet);
    let SceneGeometry::Polygon { details, .. } = &scene.item(SceneId::Object(id)).unwrap().geometry
    else {
        panic!("expected double-ellipse geometry");
    };
    assert_eq!(
        details,
        &crate::board::shape_geometry_with_style(
            ShapeKind::DoubleEllipse,
            frame,
            0.0,
            None,
            9.0,
            None,
        )
        .details
    );
}

#[test]
fn partial_rectangle_keeps_its_selection_and_connector_perimeter() {
    use crate::RectangleSides;

    let mut sheet = Sheet::new("Partial rectangle", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 100.0, 60.0));
    object.rectangle_sides = Some(RectangleSides {
        top: false,
        right: false,
        bottom: false,
        left: false,
    });
    let id = sheet.add_canvas_object(object).unwrap();
    let (layout, scene) = scene(&sheet);
    assert_eq!(
        scene.hit_test(Vec2::new(50.0, 30.0)),
        Some(SceneHit::Object(id))
    );
    let connector = CanvasConnector::new(
        Endpoint::Object(id),
        Endpoint::Point(Vec2::new(200.0, 30.0)),
    );
    assert_eq!(
        connector_path(&sheet, &layout, &connector)[0],
        Vec2::new(100.0, 30.0)
    );
}

#[test]
fn partial_rectangle_scene_bounds_include_wide_rotated_outline_caps() {
    use crate::{Color, RectangleSides};

    let frame = rect(10.0, 20.0, 100.0, 60.0);
    for rotation in [0.0, 37.0, 90.0] {
        let mut sheet = Sheet::new("Partial rectangle", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut object = CanvasObject::shape(ShapeKind::Rectangle, frame);
        object.rotation = rotation;
        object.rectangle_sides = Some(RectangleSides {
            top: true,
            right: false,
            bottom: false,
            left: false,
        });
        object.style.stroke = Some(Color::BLACK);
        object.style.stroke_width = 80.0;
        let id = sheet.add_canvas_object(object).unwrap();
        let (layout, _) = scene(&sheet);
        let object = sheet.canvas.object(id).unwrap();
        let bounds = object_bounds(&sheet, &layout, object).unwrap();
        for point in [
            Vec2::new(-30.0, -20.0),
            Vec2::new(150.0, -20.0),
            Vec2::new(150.0, 60.0),
            Vec2::new(-30.0, 60.0),
        ] {
            let point = rotate_around(point, frame.center(), rotation);
            assert!(
                bounds.contains(point),
                "{rotation} degrees: {point:?} not in {bounds:?}"
            );
        }

        let object = sheet.canvas.object_mut(id).unwrap();
        object.rectangle_sides = Some(RectangleSides {
            top: false,
            right: false,
            bottom: false,
            left: false,
        });
        let (layout, _) = scene(&sheet);
        let object = sheet.canvas.object(id).unwrap();
        assert_eq!(
            object_bounds(&sheet, &layout, object).unwrap(),
            rotated_bounds(frame, rotation)
        );
    }
}

#[test]
fn edge_to_edge_ends_meet_a_bent_target_on_its_path() {
    let mut sheet = Sheet::new("Edges", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let target = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(280.0, 280.0, 40.0, 40.0),
        Band::Above,
    );
    let elbow = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector {
            route: ConnectorRoute::Elbow,
            ..CanvasConnector::new(
                Endpoint::Point(Vec2::new(0.0, 600.0)),
                Endpoint::Object(target),
            )
        }))
        .unwrap();
    let dependent = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(elbow),
            Endpoint::Point(Vec2::new(-300.0, 0.0)),
        )))
        .unwrap();
    let (layout, _) = scene(&sheet);
    let path = |id| match &sheet.canvas.object(id).unwrap().kind {
        CanvasObjectKind::Connector(c) => connector_path(&sheet, &layout, c),
        _ => unreachable!(),
    };
    let bent = path(elbow);
    assert!(bent.len() > 2, "{bent:?}");
    let start = path(dependent)[0];
    // Halfway along the route; today's routes are point-symmetric, but waypoint
    // routes (Phase 8) would put a bounding-box centre off the path.
    assert!(
        distance_to_polyline(start, &bent) < 0.5,
        "{start:?} {bent:?}"
    );
    assert_eq!(Some(start), crate::board::polyline_midpoint(&bent));
}

#[test]
fn polyline_midpoint_follows_length() {
    let points = [Vec2::ZERO, Vec2::new(100.0, 0.0), Vec2::new(100.0, 300.0)];
    assert_eq!(
        crate::board::polyline_midpoint(&points),
        Some(Vec2::new(100.0, 100.0))
    );
    assert_eq!(crate::board::polyline_midpoint(&[]), None);
    assert_eq!(
        crate::board::polyline_midpoint(&[Vec2::new(4.0, 5.0)]),
        Some(Vec2::new(4.0, 5.0))
    );
}

#[test]
fn connectors_follow_resolved_connector_targets_and_detect_cycles() {
    let mut sheet = Sheet::new("Connectors", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let target = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(100.0, 0.0, 20.0, 20.0),
        Band::Above,
    );
    let base = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(target),
            Endpoint::Point(Vec2::new(300.0, 10.0)),
        )))
        .unwrap();
    let dependent = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(base),
            Endpoint::Point(Vec2::new(400.0, 100.0)),
        )))
        .unwrap();

    let (layout, built_scene) = scene(&sheet);
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(dependent).unwrap().kind
    else {
        unreachable!();
    };
    let path = connector_path(&sheet, &layout, connector);
    assert_eq!(path[0], Vec2::new(210.0, 10.0));
    assert_eq!(
        built_scene
            .item(SceneId::Object(dependent))
            .unwrap()
            .geometry,
        SceneGeometry::Polyline {
            points: path,
            width: connector.width,
            markers: [None, None],
        }
    );

    sheet
        .move_canvas_objects(&[target], Vec2::new(100.0, 0.0))
        .unwrap();
    let (layout, _) = scene(&sheet);
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(dependent).unwrap().kind
    else {
        unreachable!();
    };
    assert_eq!(
        connector_path(&sheet, &layout, connector)[0],
        Vec2::new(260.0, 10.0)
    );
    assert!(validate_connector_cycles(&sheet).is_ok());

    let mut deleted = sheet.clone();
    deleted.remove_canvas_objects(&[base]).unwrap();
    let CanvasObjectKind::Connector(connector) = &deleted.canvas.object(dependent).unwrap().kind
    else {
        unreachable!();
    };
    assert_eq!(connector.from, Endpoint::Point(Vec2::new(260.0, 10.0)));

    let mut doc = crate::Document::new(crate::Workbook::new(sheet.clone()));
    doc.edit_sheet("Remove", |sheet| {
        sheet.canvas.objects.retain(|object| object.id != base);
        Ok(())
    })
    .unwrap();
    let CanvasObjectKind::Connector(connector) =
        &doc.sheet().canvas.object(dependent).unwrap().kind
    else {
        unreachable!();
    };
    assert_eq!(connector.from, Endpoint::Point(Vec2::new(260.0, 10.0)));
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &sheet);

    let CanvasObjectKind::Connector(connector) = &mut sheet.canvas.object_mut(base).unwrap().kind
    else {
        unreachable!();
    };
    connector.from = Endpoint::Object(dependent);
    assert!(validate_connector_cycles(&sheet).is_err());
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(dependent).unwrap().kind
    else {
        unreachable!();
    };
    assert!(connector_path(&sheet, &layout, connector).is_empty());
}

#[test]
fn long_connector_chains_resolve_without_recursion() {
    let mut sheet = Sheet::new("Connectors", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut previous = Endpoint::Point(Vec2::ZERO);
    for n in 0..4_000 {
        let id = CanvasObjectId(n + 10);
        let mut object = CanvasObject::connector(CanvasConnector::new(
            previous,
            Endpoint::Point(Vec2::new(100.0, 100.0)),
        ));
        object.id = id;
        sheet.canvas.objects.push(object);
        previous = Endpoint::Object(id);
    }
    assert!(validate_connector_cycles(&sheet).is_ok());
    let (_, scene) = scene(&sheet);
    assert_eq!(scene.items.len(), 4_000);
    assert!(scene.item(SceneId::Object(CanvasObjectId(4_009))).is_some());
}

#[test]
fn hidden_root_is_excluded_everywhere() {
    let mut s = Sheet::new("Hidden", "");
    s.root_mode = SheetRootMode::Hidden;
    let f = s.add_floating("F", Vec2::new(400.0, 400.0));
    let (layout, sc) = scene(&s);
    assert!(layout.rect(s.root).is_none());
    assert!(sc.item(SceneId::Topic(s.root)).is_none());
    assert_eq!(sc.hit_test(Vec2::ZERO), None);
    assert_eq!(
        sc.objects_in(rect(-1000.0, -1000.0, 2000.0, 2000.0)),
        vec![SceneHit::Topic(f)]
    );
    let b = sc.bounds().unwrap();
    assert!(!b.contains(Vec2::ZERO), "{b:?}");
    assert!(!layout.bounds.contains(Vec2::ZERO), "{:?}", layout.bounds);
    assert_eq!(topic_anchor_rect(&s, &layout, s.root), None);

    // Empty hidden board: no items, no bounds, no panic.
    let mut empty = Sheet::new("Empty", "");
    empty.root_mode = SheetRootMode::Hidden;
    let (_, sc) = scene(&empty);
    assert!(sc.items.is_empty());
    assert_eq!(sc.bounds(), None);
    assert_eq!(sc.hit_test(Vec2::ZERO), None);
}

#[test]
fn objects_in_and_bounds_include_canvas_objects() {
    let mut s = Sheet::new("Area", "Root");
    let far = shape(
        &mut s,
        ShapeKind::Star,
        rect(5000.0, 5000.0, 100.0, 100.0),
        Band::Below,
    );
    let (layout, sc) = scene(&s);
    assert!(layout.bounds.contains(Vec2::new(5050.0, 5050.0)));
    let b = sc.bounds().unwrap();
    assert!(
        b.contains(Vec2::new(5050.0, 5050.0)) && b.max().x > 5090.0,
        "{b:?}"
    );
    assert_eq!(
        sc.objects_in(rect(4900.0, 4900.0, 150.0, 150.0)),
        vec![SceneHit::Object(far)]
    );
    let all = sc.objects_in(rect(-100.0, -100.0, 6000.0, 6000.0));
    assert_eq!(all, vec![SceneHit::Object(far), SceneHit::Topic(s.root)]);
}

#[test]
fn rotated_card_topic_hit_test() {
    let mut s = Sheet::new("Card", "Root");
    let mut card = crate::board::Card::new(crate::board::CardKind::Sticky, Vec2::new(200.0, 20.0));
    card.rotation = 90.0;
    let id = s.add_card("C", Vec2::new(1000.0, 0.0), card).unwrap();
    let (_, sc) = scene(&s);
    assert_eq!(
        sc.hit_test(Vec2::new(1000.0, 90.0)),
        Some(SceneHit::Topic(id))
    );
    assert_eq!(sc.hit_test(Vec2::new(1090.0, 0.0)), None);
}
