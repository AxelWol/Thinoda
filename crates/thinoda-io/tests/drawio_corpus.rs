//! Validates the draw.io codec against real diagrams and templates.
//!
//! Point `THINODA_DRAWIO_CORPUS` at a folder of `.drawio`/`.xml` files, or keep
//! them in `docs/draw-io-examples` (not committed; the test skips when absent).

use std::path::{Path, PathBuf};
use thinoda_core::{
    Color, ConnectorPort, Rect, RectangleSides, ShapeKind, Sheet, Vec2, Workbook,
    board::{CanvasObjectKind, ConnectorMarkerKind, ConnectorRoute},
};
use thinoda_io::{ExportLoss, ExportOptions, Format, IoError};

fn corpus_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("THINODA_DRAWIO_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/draw-io-examples")
        });
    dir.is_dir().then_some(dir)
}

fn items(book: &Workbook) -> usize {
    book.sheets
        .iter()
        .map(|sheet| sheet.topics.len().saturating_sub(1) + sheet.canvas.objects.len())
        .sum()
}

fn waypoint_paths(sheet: &Sheet) -> Vec<(ConnectorRoute, &[Vec2])> {
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|object| match &object.kind {
            CanvasObjectKind::Connector(connector) if !connector.waypoints.is_empty() => {
                Some((connector.route, connector.waypoints.as_ref()))
            }
            _ => None,
        })
        .collect()
}

fn fixed_ports(sheet: &Sheet) -> Vec<ConnectorPort> {
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|object| match &object.kind {
            CanvasObjectKind::Connector(connector) => {
                Some([connector.from_port, connector.to_port])
            }
            _ => None,
        })
        .flatten()
        .flatten()
        .collect()
}

fn check_fixed_ports(
    before: &Workbook,
    after: &Workbook,
    expected_detached: usize,
) -> Result<(), String> {
    let mut detached = 0;
    for (index, (original, restored)) in before.sheets.iter().zip(&after.sheets).enumerate() {
        let mut remaining = fixed_ports(restored);
        for port in fixed_ports(original) {
            if let Some(found) = remaining.iter().position(|other| *other == port) {
                remaining.swap_remove(found);
            } else {
                detached += 1;
            }
        }
        if !remaining.is_empty() {
            return Err(format!(
                "page {} gained {} or changed fixed connector ports",
                index + 1,
                remaining.len()
            ));
        }
    }
    if detached != expected_detached {
        return Err(format!(
            "fixed connector ports lost {detached}; export reported {expected_detached} detached"
        ));
    }
    Ok(())
}

#[test]
fn fixed_port_corpus_check_detects_changes_and_unreported_detachments() {
    let mut before = Workbook::default();
    let target = before
        .active_mut()
        .add_canvas_object(thinoda_core::CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        ))
        .unwrap();
    let mut connector = thinoda_core::CanvasConnector::new(
        thinoda_core::Endpoint::Object(target),
        thinoda_core::Endpoint::Point(Vec2::new(200.0, 0.0)),
    );
    connector.from_port = Some(ConnectorPort {
        position: Vec2::new(1.0, 0.5),
        offset: Vec2::ZERO,
        perimeter: true,
    });
    let edge = before
        .active_mut()
        .add_canvas_object(thinoda_core::CanvasObject::connector(connector))
        .unwrap();
    let mut after = before.clone();
    if let CanvasObjectKind::Connector(connector) =
        &mut after.active_mut().canvas.object_mut(edge).unwrap().kind
    {
        connector.from_port.as_mut().unwrap().position.y = 0.25;
    }
    assert!(check_fixed_ports(&before, &after, 0).is_err());
    if let CanvasObjectKind::Connector(connector) =
        &mut after.active_mut().canvas.object_mut(edge).unwrap().kind
    {
        connector.from_port = None;
    }
    assert!(check_fixed_ports(&before, &after, 0).is_err());
    assert!(check_fixed_ports(&before, &after, 1).is_ok());
}

fn check_waypoints(before: &Workbook, after: &Workbook) -> Result<(), String> {
    let close = |a: f32, b: f32| {
        (a - b).abs() <= 0.001 + 2.0 * f32::EPSILON * a.abs().max(b.abs()).max(1.0)
    };
    for (index, (original, restored)) in before.sheets.iter().zip(&after.sheets).enumerate() {
        let paths = waypoint_paths(original);
        let mut remaining = waypoint_paths(restored);
        for (route, points) in paths {
            let Some(found) = remaining.iter().position(|(other_route, other)| {
                route == *other_route
                    && points.len() == other.len()
                    && points
                        .iter()
                        .zip(*other)
                        .all(|(a, b)| close(a.x, b.x) && close(a.y, b.y))
            }) else {
                return Err(format!(
                    "page {} lost a {route:?} path with {} waypoints",
                    index + 1,
                    points.len()
                ));
            };
            remaining.swap_remove(found);
        }
        if !remaining.is_empty() {
            return Err(format!(
                "page {} gained {} waypoint paths",
                index + 1,
                remaining.len()
            ));
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
struct MarkerSnapshot {
    start: Option<ConnectorMarkerKind>,
    end: Option<ConnectorMarkerKind>,
    start_size: Option<f32>,
    end_size: Option<f32>,
    start_unfilled: bool,
    end_unfilled: bool,
}

fn markers(sheet: &Sheet) -> Vec<MarkerSnapshot> {
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|object| match &object.kind {
            CanvasObjectKind::Connector(connector)
                if connector.start_marker.is_some() || connector.end_marker.is_some() =>
            {
                Some(MarkerSnapshot {
                    start: connector.start_marker.map(|marker| marker.kind),
                    end: connector.end_marker.map(|marker| marker.kind),
                    start_size: connector.start_marker_size,
                    end_size: connector.end_marker_size,
                    start_unfilled: connector.start_marker_unfilled,
                    end_unfilled: connector.end_marker_unfilled,
                })
            }
            _ => None,
        })
        .collect()
}

fn check_markers(before: &Workbook, after: &Workbook) -> Result<(), String> {
    for (index, (original, restored)) in before.sheets.iter().zip(&after.sheets).enumerate() {
        let mut remaining = markers(restored);
        for marker in markers(original) {
            let Some(found) = remaining.iter().position(|other| *other == marker) else {
                return Err(format!(
                    "page {} lost connector markers {marker:?}",
                    index + 1
                ));
            };
            remaining.swap_remove(found);
        }
        if !remaining.is_empty() {
            return Err(format!(
                "page {} gained {} connector markers",
                index + 1,
                remaining.len()
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct PartialRectangleSnapshot {
    frame: Rect,
    rotation: f32,
    sides: RectangleSides,
}

fn partial_rectangles(sheet: &Sheet) -> Vec<PartialRectangleSnapshot> {
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|object| {
            object
                .rectangle_sides
                .map(|sides| PartialRectangleSnapshot {
                    frame: object.rect,
                    rotation: object.rotation,
                    sides,
                })
        })
        .collect()
}

fn check_partial_rectangles(before: &Workbook, after: &Workbook) -> Result<(), String> {
    let close = |a: f32, b: f32| {
        (a - b).abs() <= 0.001 + 2.0 * f32::EPSILON * a.abs().max(b.abs()).max(1.0)
    };
    for (index, (original, restored)) in before.sheets.iter().zip(&after.sheets).enumerate() {
        let mut remaining = partial_rectangles(restored);
        for rectangle in partial_rectangles(original) {
            let Some(found) = remaining.iter().position(|other| {
                rectangle.sides == other.sides
                    && close(rectangle.frame.min.x, other.frame.min.x)
                    && close(rectangle.frame.min.y, other.frame.min.y)
                    && close(rectangle.frame.size.x, other.frame.size.x)
                    && close(rectangle.frame.size.y, other.frame.size.y)
                    && close(rectangle.rotation, other.rotation)
            }) else {
                return Err(format!(
                    "page {} lost partial rectangle {rectangle:?}",
                    index + 1
                ));
            };
            remaining.swap_remove(found);
        }
        if !remaining.is_empty() {
            return Err(format!(
                "page {} gained {} partial rectangles",
                index + 1,
                remaining.len()
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct DoubleEllipseSnapshot {
    frame: Rect,
    rotation: f32,
    margin: Option<f32>,
}

fn double_ellipses(sheet: &Sheet) -> Vec<DoubleEllipseSnapshot> {
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|object| {
            matches!(
                object.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::DoubleEllipse,
                    ..
                }
            )
            .then_some(DoubleEllipseSnapshot {
                frame: object.rect,
                rotation: object.rotation,
                margin: object.double_ellipse_margin,
            })
        })
        .collect()
}

fn check_double_ellipses(before: &Workbook, after: &Workbook) -> Result<(), String> {
    let close = |a: f32, b: f32| {
        (a - b).abs() <= 0.001 + 2.0 * f32::EPSILON * a.abs().max(b.abs()).max(1.0)
    };
    for (index, (original, restored)) in before.sheets.iter().zip(&after.sheets).enumerate() {
        let mut remaining = double_ellipses(restored);
        for ellipse in double_ellipses(original) {
            let Some(found) = remaining.iter().position(|other| {
                (match (ellipse.margin, other.margin) {
                    (Some(a), Some(b)) => close(a, b),
                    (None, None) => true,
                    _ => false,
                }) && close(ellipse.frame.min.x, other.frame.min.x)
                    && close(ellipse.frame.min.y, other.frame.min.y)
                    && close(ellipse.frame.size.x, other.frame.size.x)
                    && close(ellipse.frame.size.y, other.frame.size.y)
                    && close(ellipse.rotation, other.rotation)
            }) else {
                return Err(format!(
                    "page {} lost double ellipse {ellipse:?}",
                    index + 1
                ));
            };
            remaining.swap_remove(found);
        }
        if !remaining.is_empty() {
            return Err(format!(
                "page {} gained {} double ellipses",
                index + 1,
                remaining.len()
            ));
        }
    }
    Ok(())
}

#[test]
fn double_ellipse_corpus_check_detects_changed_insets() {
    let mut book = Workbook::default();
    let mut ellipse = thinoda_core::CanvasObject::shape(
        ShapeKind::DoubleEllipse,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
    );
    ellipse.double_ellipse_margin = Some(8.5);
    let id = book.active_mut().add_canvas_object(ellipse).unwrap();
    let mut changed = book.clone();
    assert!(check_double_ellipses(&book, &changed).is_ok());
    changed
        .active_mut()
        .canvas
        .object_mut(id)
        .unwrap()
        .double_ellipse_margin = None;
    assert!(check_double_ellipses(&book, &changed).is_err());
}

fn check(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let format = thinoda_io::detect_format(path, &bytes);
    if format != Some(Format::DrawIo) {
        return Err(format!("detected as {format:?}"));
    }
    let report = thinoda_io::import_bytes_with_report(Format::DrawIo, &bytes)
        .map_err(|e| format!("import: {e}"))?;
    let book = report.workbook;
    book.validate()
        .map_err(|e| format!("invalid workbook: {e}"))?;
    let xml = std::str::from_utf8(&bytes).map_err(|e| format!("source is not UTF-8: {e}"))?;
    let link = format!(
        "#R{}",
        thinoda_io::drawio_clipboard::encode_uri_component(xml)
    );
    let linked = thinoda_io::import_drawio_link(&link).map_err(|e| format!("link import: {e}"))?;
    if linked.workbook != book || linked.warnings != report.warnings {
        return Err("link import differs from the original file import".into());
    }
    let count = items(&book);
    if count == 0 {
        return Err("no items imported".into());
    }
    let opts = ExportOptions::default();

    let losses = thinoda_io::export_losses(&book, Format::DrawIo, &opts);
    let attachment_losses: Vec<_> = losses
        .iter()
        .filter(|loss| {
            matches!(
                loss,
                ExportLoss::ConnectorAttachmentsDetached(_)
                    | ExportLoss::FixedConnectorAttachmentsDetached(_)
                    | ExportLoss::DrawioFixedConnectorPositionApproximated(_)
            )
        })
        .cloned()
        .collect();
    let strict = thinoda_io::export_bytes(&book, Format::DrawIo, &opts);
    let drawio = match (attachment_losses.as_slice(), strict) {
        ([], Ok(bytes)) => bytes,
        (
            [ExportLoss::ConnectorAttachmentsDetached(expected)],
            Err(IoError::DrawioConnectorAttachmentsNeedConfirmation(actual)),
        ) if expected == &actual => {
            thinoda_io::export_bytes_with_consent(&book, Format::DrawIo, &opts, true)
                .map_err(|e| format!("consented draw.io export: {e}"))?
        }
        (expected, Err(IoError::DrawioFixedConnectorAttachmentsNeedConfirmation(actual)))
            if expected == actual =>
        {
            thinoda_io::export_bytes_with_consent(&book, Format::DrawIo, &opts, true)
                .map_err(|e| format!("consented draw.io export: {e}"))?
        }
        (expected, Ok(_)) => {
            return Err(format!(
                "strict draw.io export accepted attachment losses {expected:?}"
            ));
        }
        (expected, Err(error)) => {
            return Err(format!(
                "strict draw.io export reported {error:?} instead of {expected:?}"
            ));
        }
    };
    let again = thinoda_io::import_bytes_with_report(Format::DrawIo, &drawio)
        .map_err(|e| format!("draw.io re-import: {e}"))?
        .workbook;
    if again.sheets.len() != book.sheets.len() {
        return Err(format!(
            "round trip pages {} -> {}",
            book.sheets.len(),
            again.sheets.len()
        ));
    }
    for (index, (original, restored)) in book.sheets.iter().zip(&again.sheets).enumerate() {
        if original.board != restored.board {
            return Err(format!("page {} changed its grid settings", index + 1));
        }
        if original.theme.background != restored.theme.background {
            return Err(format!("page {} changed its background", index + 1));
        }
    }
    let round_trip = items(&again);
    if round_trip < count {
        return Err(format!("round trip lost items {count} -> {round_trip}"));
    }
    check_waypoints(&book, &again)?;
    check_markers(&book, &again)?;
    check_partial_rectangles(&book, &again)?;
    check_double_ellipses(&book, &again)?;
    let detached = attachment_losses
        .iter()
        .find_map(|loss| match loss {
            ExportLoss::FixedConnectorAttachmentsDetached(count) => Some(*count),
            _ => None,
        })
        .unwrap_or(0);
    check_fixed_ports(&book, &again, detached)?;

    let native = thinoda_io::export_bytes(&book, Format::Thinoda, &opts)
        .map_err(|e| format!("native export: {e}"))?;
    let reopened = thinoda_io::import_bytes(Format::Thinoda, &native)
        .map_err(|e| format!("native re-import: {e}"))?;
    if items(&reopened) != count {
        return Err("native round trip changed the item count".into());
    }
    if reopened != book {
        return Err("native round trip changed workbook content".into());
    }
    thinoda_io::export_bytes(&book, Format::Svg, &opts).map_err(|e| format!("svg: {e}"))?;

    let text = String::from_utf8_lossy(&bytes);
    if thinoda_io::drawio_clipboard::looks_like_drawio(&text) {
        use thinoda_io::ImportWarning;
        use thinoda_io::drawio_clipboard::DrawioPasteLoss;

        let pasted =
            thinoda_io::drawio_clipboard::decode(&text).map_err(|e| format!("clipboard: {e}"))?;
        let mut expected = Vec::new();
        if !book.sheets[0].zones.is_empty() {
            expected.push(DrawioPasteLoss::ContainerFrames(book.sheets[0].zones.len()));
        }
        if book.sheets.len() > 1 {
            expected.push(DrawioPasteLoss::AdditionalPages(book.sheets.len() - 1));
        }
        if pasted.losses != expected {
            return Err(format!(
                "clipboard loss report {:?} differs from {expected:?}",
                pasted.losses
            ));
        }
        if !expected.is_empty() && !pasted.warnings.contains(&thinoda_io::ImportWarning::Other) {
            return Err("clipboard omissions were not reported as a warning".into());
        }
        if pasted.warnings.iter().any(|warning| {
            !report.warnings.contains(warning)
                && !(*warning == ImportWarning::Other && !expected.is_empty())
        }) {
            return Err(format!(
                "clipboard introduced an unrelated import warning: {:?}",
                pasted.warnings
            ));
        }
        if book.sheets.len() == 1 {
            let mut first_page_warnings = report.warnings.clone();
            first_page_warnings.retain(|warning| {
                !matches!(
                    warning,
                    ImportWarning::DrawioGridAdjusted | ImportWarning::DrawioBackgroundAdjusted
                )
            });
            if !expected.is_empty() && !first_page_warnings.contains(&ImportWarning::Other) {
                first_page_warnings.push(ImportWarning::Other);
                first_page_warnings.sort_unstable();
            }
            if pasted.warnings != first_page_warnings {
                return Err(format!(
                    "clipboard warnings {:?} differ from first-page warnings {first_page_warnings:?}",
                    pasted.warnings
                ));
            }
        }
    }
    Ok(format!(
        "{} page(s), {count} item(s), {} waypoint edge(s), {} marker edge(s), {} sized marker end(s), {} partial rectangle(s), {} double ellipse(s), {} fixed port(s), {} warning kind(s)",
        book.sheets.len(),
        book.sheets
            .iter()
            .map(|sheet| waypoint_paths(sheet).len())
            .sum::<usize>(),
        book.sheets
            .iter()
            .map(|sheet| markers(sheet).len())
            .sum::<usize>(),
        book.sheets
            .iter()
            .flat_map(markers)
            .map(|marker| {
                usize::from(marker.start_size.is_some()) + usize::from(marker.end_size.is_some())
            })
            .sum::<usize>(),
        book.sheets
            .iter()
            .map(|sheet| partial_rectangles(sheet).len())
            .sum::<usize>(),
        book.sheets
            .iter()
            .map(|sheet| double_ellipses(sheet).len())
            .sum::<usize>(),
        book.sheets
            .iter()
            .map(|sheet| fixed_ports(sheet).len())
            .sum::<usize>(),
        report.warnings.len()
    ))
}

#[test]
fn real_drawio_diagrams_import_round_trip_and_render() {
    let Some(dir) = corpus_dir() else {
        eprintln!("draw.io corpus not found; skipping");
        return;
    };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e, "drawio" | "xml" | "dio"))
        })
        .collect();
    files.sort();
    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy();
        match std::panic::catch_unwind(|| check(path)) {
            Ok(Ok(summary)) => eprintln!("ok   {name}: {summary}"),
            Ok(Err(error)) => failures.push(format!("{name}: {error}")),
            Err(_) => failures.push(format!("{name}: panicked")),
        }
    }
    eprintln!("{} file(s), {} failure(s)", files.len(), failures.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn supplied_diagrams_follow_their_grid_and_snap_settings() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    for (name, grid, snap) in [
        (
            "requirements-flow.drawio",
            thinoda_core::GridKind::Lines,
            true,
        ),
        ("horse-labelled.drawio", thinoda_core::GridKind::None, false),
        (
            "board-visual-tutorial.drawio",
            thinoda_core::GridKind::None,
            false,
        ),
    ] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(path).unwrap();
        let report = thinoda_io::import_bytes_with_report(Format::DrawIo, &bytes).unwrap();
        for sheet in &report.workbook.sheets {
            assert_eq!(
                (sheet.board.grid, sheet.board.grid_size, sheet.board.snap),
                (grid, 10.0, snap),
                "{name}: {}",
                sheet.title
            );
        }
    }
}

#[test]
fn supplied_requirements_flow_keeps_its_annotation_brackets() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    let path = dir.join("requirements-flow.drawio");
    if !path.is_file() {
        return;
    }
    let bytes = std::fs::read(path).unwrap();
    let report = thinoda_io::import_bytes_with_report(Format::DrawIo, &bytes).unwrap();
    let sides = RectangleSides {
        top: true,
        right: false,
        bottom: true,
        left: true,
    };
    let brackets: Vec<_> = report
        .workbook
        .sheets
        .iter()
        .flat_map(|sheet| &sheet.canvas.objects)
        .filter(|object| object.rectangle_sides == Some(sides))
        .collect();
    for (height, rotation) in [(660.0, 0.0), (480.0, 0.0), (480.0, 180.0)] {
        let bracket = brackets
            .iter()
            .find(|object| {
                object.rect.size == Vec2::new(50.0, height) && object.rotation == rotation
            })
            .expect("the supplied annotation bracket should remain editable");
        assert_eq!(bracket.style.fill, Some(Color::TRANSPARENT));
        assert_eq!(bracket.style.stroke, Some(Color::BLACK));
        assert_eq!(bracket.style.stroke_width, 2.0);
    }
    report.workbook.validate().unwrap();
}

#[test]
fn supplied_gantt_processes_remain_editable_with_a_warning() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    for name in ["gantt_1.xml", "gantt_1 (1).xml"] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(path).unwrap();
        let report = thinoda_io::import_bytes_with_report(Format::DrawIo, &bytes).unwrap();
        assert!(
            report
                .warnings
                .contains(&thinoda_io::ImportWarning::Formatting),
            "{name}"
        );
        let count = |book: &Workbook| {
            book.sheets
                .iter()
                .flat_map(|sheet| &sheet.canvas.objects)
                .filter(|object| {
                    matches!(
                        object.kind,
                        CanvasObjectKind::Shape {
                            shape: ShapeKind::Process,
                            ..
                        }
                    )
                })
                .count()
        };
        assert_eq!(count(&report.workbook), 22, "{name}");
        let exported =
            thinoda_io::export_bytes(&report.workbook, Format::DrawIo, &ExportOptions::default())
                .unwrap();
        let reopened = thinoda_io::import_bytes_with_report(Format::DrawIo, &exported).unwrap();
        assert_eq!(count(&reopened.workbook), 22, "{name}");
        reopened.workbook.validate().unwrap();
    }
}

#[test]
fn supplied_floorplan_chair_placeholders_are_explicitly_reported() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    for (name, minimum) in [
        ("seating_plan_3.xml", 481),
        ("evacuation_plan_4.xml", 85),
        ("lan_plan_2.xml", 23),
        ("Floorplan.xml", 7),
    ] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(path).unwrap();
        let report = thinoda_io::import_bytes_with_report(Format::DrawIo, &bytes).unwrap();
        assert!(
            report
                .warnings
                .contains(&thinoda_io::ImportWarning::UnsupportedDrawioShapes),
            "{name}"
        );
        let placeholders = report
            .workbook
            .sheets
            .iter()
            .flat_map(|sheet| &sheet.canvas.objects)
            .filter(|object| {
                matches!(
                    object.kind,
                    CanvasObjectKind::Shape {
                        shape: ShapeKind::Rectangle,
                        ..
                    }
                ) && object.label().is_empty()
                    && (object.rect.size == Vec2::new(40.0, 43.0)
                        || object.rect.size == Vec2::new(43.0, 40.0))
            })
            .count();
        assert!(placeholders >= minimum, "{name}: {placeholders}");
        report.workbook.validate().unwrap();
    }
}

#[test]
fn supplied_uml_activity_keeps_its_rgb_colors() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    let path = dir.join("uml-activity-example.drawio");
    if !path.is_file() {
        return;
    }
    let bytes = std::fs::read(path).unwrap();
    let book = thinoda_io::import_bytes_with_report(Format::DrawIo, &bytes)
        .unwrap()
        .workbook;
    let object = book
        .sheets
        .iter()
        .flat_map(|sheet| &sheet.canvas.objects)
        .find(|object| object.label().contains("Guard statement"))
        .expect("the supplied UML activity should contain its guard statement");
    assert_eq!(object.style.fill, Some(Color::WHITE));
    assert_eq!(object.style.stroke, Some(Color::BLACK));
    assert_eq!(object.style.text_color, Some(Color::BLACK));
    book.validate().unwrap();
}

#[test]
fn supplied_uml_activity_note_is_editable_with_a_warning() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    let path = dir.join("uml-activity-example.drawio");
    if !path.is_file() {
        return;
    }
    let report =
        thinoda_io::import_bytes_with_report(Format::DrawIo, &std::fs::read(path).unwrap())
            .unwrap();
    assert!(
        report
            .warnings
            .contains(&thinoda_io::ImportWarning::DrawioFoldedNotesApproximated)
    );
    let note = report
        .workbook
        .sheets
        .iter()
        .flat_map(|sheet| sheet.topics.values())
        .find(|topic| topic.title == "Note")
        .expect("the supplied UML activity should contain its note");
    let card = note.card.expect("the note should become an editable card");
    assert_eq!(card.kind, thinoda_core::CardKind::Sticky);
    assert_eq!(card.size, Vec2::new(80.0, 40.0));
    assert_eq!(note.position, Some(Vec2::new(612.5, 100.0)));
    report.workbook.validate().unwrap();
    let native =
        thinoda_io::export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default())
            .unwrap();
    assert_eq!(
        thinoda_io::import_bytes(Format::Thinoda, &native).unwrap(),
        report.workbook
    );
}

#[test]
fn supplied_uml_package_keeps_its_thin_diamond_markers() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    let path = dir.join("uml-package-example.drawio");
    if !path.is_file() {
        return;
    }
    let bytes = std::fs::read(path).unwrap();
    let source = std::str::from_utf8(&bytes).unwrap();
    let expected = source.matches("startArrow=diamondThin").count()
        + source.matches("endArrow=diamondThin").count();
    assert!(expected > 0, "the UML example has no thin diamond markers");
    let book = thinoda_io::import_bytes_with_report(Format::DrawIo, &bytes)
        .unwrap()
        .workbook;
    let count = book
        .sheets
        .iter()
        .flat_map(markers)
        .map(|marker| {
            usize::from(marker.start == Some(ConnectorMarkerKind::DiamondThin))
                + usize::from(marker.end == Some(ConnectorMarkerKind::DiamondThin))
        })
        .sum::<usize>();
    assert_eq!(count, expected);
    let exported =
        thinoda_io::export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap();
    let reopened = thinoda_io::import_bytes_with_report(Format::DrawIo, &exported)
        .unwrap()
        .workbook;
    check_markers(&book, &reopened).unwrap();
}

#[test]
fn supplied_diagrams_keep_their_oval_markers() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    for name in [
        "infographic-rockpool.drawio",
        "uml-component-example.drawio",
    ] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }

        let bytes = std::fs::read(path).unwrap();
        let source = std::str::from_utf8(&bytes).unwrap();
        let expected =
            source.matches("startArrow=oval").count() + source.matches("endArrow=oval").count();
        assert!(expected > 0, "{name} has no oval markers");
        let book = thinoda_io::import_bytes_with_report(Format::DrawIo, &bytes)
            .unwrap()
            .workbook;
        let count = book
            .sheets
            .iter()
            .flat_map(markers)
            .map(|marker| {
                usize::from(marker.start == Some(ConnectorMarkerKind::Oval))
                    + usize::from(marker.end == Some(ConnectorMarkerKind::Oval))
            })
            .sum::<usize>();
        assert_eq!(count, expected, "{name}");
        let exported =
            thinoda_io::export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap();
        let reopened = thinoda_io::import_bytes_with_report(Format::DrawIo, &exported)
            .unwrap()
            .workbook;
        check_markers(&book, &reopened).unwrap();
    }
}

#[test]
fn supplied_uml_component_keeps_its_half_circle_markers() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    let path = dir.join("uml-component-example.drawio");
    if !path.is_file() {
        return;
    }
    let bytes = std::fs::read(path).unwrap();
    let source = std::str::from_utf8(&bytes).unwrap();
    let expected = source.matches("startArrow=halfCircle").count()
        + source.matches("endArrow=halfCircle").count();
    assert!(expected > 0, "the UML example has no half-circle markers");
    let book = thinoda_io::import_bytes_with_report(Format::DrawIo, &bytes)
        .unwrap()
        .workbook;
    let count = book
        .sheets
        .iter()
        .flat_map(markers)
        .map(|marker| {
            usize::from(marker.start == Some(ConnectorMarkerKind::HalfCircle))
                + usize::from(marker.end == Some(ConnectorMarkerKind::HalfCircle))
        })
        .sum::<usize>();
    assert_eq!(count, expected);
    let exported =
        thinoda_io::export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap();
    let reopened = thinoda_io::import_bytes_with_report(Format::DrawIo, &exported)
        .unwrap()
        .workbook;
    check_markers(&book, &reopened).unwrap();
}

#[test]
fn supplied_tutorial_with_bar_markers_keeps_them_through_conversion() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    let path = dir.join("board-visual-tutorial.drawio");
    if !path.is_file() {
        return;
    }
    let source = std::fs::read_to_string(path).unwrap();
    assert!(source.contains("endArrow=open;"));
    let edited = source.replacen(
        "endArrow=open;",
        "startArrow=baseDash;startFill=0;endArrow=dash;endFill=1;",
        1,
    );
    let book = thinoda_io::import_bytes_with_report(Format::DrawIo, edited.as_bytes())
        .unwrap()
        .workbook;
    let expected = MarkerSnapshot {
        start: Some(ConnectorMarkerKind::BaseDash),
        end: Some(ConnectorMarkerKind::Dash),
        start_size: Some(14.0),
        end_size: Some(14.0),
        start_unfilled: true,
        end_unfilled: false,
    };
    assert_eq!(
        book.sheets
            .iter()
            .flat_map(markers)
            .filter(|marker| *marker == expected)
            .count(),
        1
    );
    let exported =
        thinoda_io::export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap();
    let reopened = thinoda_io::import_bytes_with_report(Format::DrawIo, &exported)
        .unwrap()
        .workbook;
    check_markers(&book, &reopened).unwrap();
    let native =
        thinoda_io::export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(
        thinoda_io::import_bytes(Format::Thinoda, &native).unwrap(),
        book
    );
}
