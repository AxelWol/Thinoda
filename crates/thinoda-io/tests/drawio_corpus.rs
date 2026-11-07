//! Validates the draw.io codec against real diagrams and templates.
//!
//! Point `THINODA_DRAWIO_CORPUS` at a folder of `.drawio`/`.xml` files, or keep
//! them in `docs/draw-io-examples` (not committed; the test skips when absent).

use std::path::{Path, PathBuf};
use thinoda_core::{
    Sheet, Vec2, Workbook,
    board::{CanvasObjectKind, ConnectorMarkerKind, ConnectorRoute},
};
use thinoda_io::{ExportOptions, Format};

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

fn markers(sheet: &Sheet) -> Vec<(Option<ConnectorMarkerKind>, Option<ConnectorMarkerKind>)> {
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|object| match &object.kind {
            CanvasObjectKind::Connector(connector)
                if connector.start_marker.is_some() || connector.end_marker.is_some() =>
            {
                Some((
                    connector.start_marker.map(|marker| marker.kind),
                    connector.end_marker.map(|marker| marker.kind),
                ))
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

    let drawio = thinoda_io::export_bytes(&book, Format::DrawIo, &opts)
        .map_err(|e| format!("draw.io export: {e}"))?;
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
    let round_trip = items(&again);
    if round_trip < count {
        return Err(format!("round trip lost items {count} -> {round_trip}"));
    }
    check_waypoints(&book, &again)?;
    check_markers(&book, &again)?;

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
        thinoda_io::drawio_clipboard::decode(&text).map_err(|e| format!("clipboard: {e}"))?;
    }
    Ok(format!(
        "{} page(s), {count} item(s), {} waypoint edge(s), {} marker edge(s), {} warning kind(s)",
        book.sheets.len(),
        book.sheets
            .iter()
            .map(|sheet| waypoint_paths(sheet).len())
            .sum::<usize>(),
        book.sheets
            .iter()
            .map(|sheet| markers(sheet).len())
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
