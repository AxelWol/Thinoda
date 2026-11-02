//! Freeform board data must never make an exporter or importer panic (phase F0).
//! Faithful board export is a later phase; these tests only pin robustness.

use thinoda_core::{
    CanvasConnector, CanvasObject, Card, CardKind, Endpoint, InkStroke, InkTool,
    PresentationSource, ShapeKind, Sheet, SheetRootMode, Workbook,
    geometry::{Color, Rect, Vec2},
};
use thinoda_io::{ExportOptions, Format, export_bytes_with_consent, import_bytes};

fn board() -> Workbook {
    let mut s = Sheet::new("Board", "");
    s.root_mode = SheetRootMode::Hidden;
    let a = s.add_floating("Idea", Vec2::new(200.0, 0.0));
    s.add_child(a, None, "Detail");
    let card = s
        .add_card(
            "Sticky",
            Vec2::new(-200.0, 100.0),
            Card::new(CardKind::Sticky, Vec2::new(160.0, 120.0)),
        )
        .unwrap();
    let shape = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Cloud,
            Rect::from_min_size(Vec2::new(0.0, 300.0), Vec2::new(120.0, 80.0)),
        ))
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
    s.add_canvas_object(CanvasObject::connector(CanvasConnector::new(
        Endpoint::Object(shape),
        Endpoint::Topic(card),
    )))
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

#[test]
fn hidden_root_boards_export_without_panicking() {
    let mut central = board();
    central.sheets[0].root_mode = SheetRootMode::Central;
    for wb in [board(), central] {
        for &format in Format::EXPORTABLE {
            if matches!(format, Format::FimXdf2 | Format::FimXdf3) {
                continue;
            }
            let result = export_bytes_with_consent(&wb, format, &ExportOptions::default(), true);
            if let Ok(bytes) = &result {
                assert!(!bytes.is_empty(), "{format:?}");
            }
        }
    }
}

#[test]
fn native_round_trip_keeps_board_data() {
    let wb = board();
    let bytes =
        export_bytes_with_consent(&wb, Format::Thinoda, &ExportOptions::default(), false).unwrap();
    let back = import_bytes(Format::Thinoda, &bytes).unwrap();
    assert_eq!(back.sheets[0].canvas, wb.sheets[0].canvas);
    assert_eq!(back.sheets[0].root_mode, SheetRootMode::Hidden);
}
