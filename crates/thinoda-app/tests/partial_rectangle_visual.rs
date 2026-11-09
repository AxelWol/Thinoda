#![cfg(feature = "gpu-tests")]

use egui::vec2;
use egui_kittest::Harness;
use thinoda_app::app::ThinodaApp;
use thinoda_core::{
    CanvasObject, Color, Rect, RectangleSides, ShapeKind, Sheet, SheetRootMode, Vec2, Workbook,
    board::rotate_around,
};
use thinoda_platform::settings::ThemeMode;

#[test]
fn partial_rectangle_canvas_keeps_sides_rotation_and_single_alpha() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut cases = Vec::new();
    for mask in 0..16 {
        let center = Vec2::new(
            (mask % 4) as f32 * 240.0 - 360.0,
            (mask / 4) as f32 * 170.0 - 280.0,
        );
        let rect = Rect::from_center_size(center, Vec2::new(80.0, 60.0));
        let sides = RectangleSides {
            top: mask & 1 != 0,
            right: mask & 2 != 0,
            bottom: mask & 4 != 0,
            left: mask & 8 != 0,
        };
        let rotation = [0.0, 90.0, 37.0][mask % 3];
        let mut shape = CanvasObject::shape(ShapeKind::Rectangle, rect);
        shape.rectangle_sides = Some(sides);
        shape.rotation = rotation;
        shape.style.fill = Some(Color::TRANSPARENT);
        shape.style.stroke = Some(Color::rgba(0, 0, 0, 128));
        shape.style.stroke_width = 8.0;
        sheet.add_canvas_object(shape).unwrap();
        cases.push((rect, rotation, sides));
    }

    for (center, size, width) in [
        (Vec2::new(0.0, 380.0), Vec2::new(20.0, 20.0), 30.0),
        (Vec2::new(-240.0, 380.0), Vec2::new(0.0, 40.0), 8.0),
        (Vec2::new(240.0, 380.0), Vec2::new(40.0, 0.0), 8.0),
    ] {
        let mut shape =
            CanvasObject::shape(ShapeKind::Rectangle, Rect::from_center_size(center, size));
        shape.rectangle_sides = Some(RectangleSides::default());
        shape.style.fill = Some(Color::TRANSPARENT);
        shape.style.stroke = Some(Color::rgba(0, 0, 0, 128));
        shape.style.stroke_width = width;
        sheet.add_canvas_object(shape).unwrap();
    }

    let mut app = ThinodaApp::headless();
    app.settings.theme = ThemeMode::Light;
    app.settings.show_format_panel = false;
    app.new_doc(Workbook::new(sheet));
    let mut harness = Harness::builder()
        .wgpu()
        .with_size(vec2(1200.0, 1100.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    harness.run_steps(6);
    {
        let view = &mut harness.state_mut().doc_mut().unwrap().view;
        view.zoom = 1.0;
        view.offset = egui::Vec2::ZERO;
    }
    harness.run_steps(3);
    let image = harness.render().expect("partial rectangle render");
    let view = &harness.state().doc().unwrap().view;
    let origin = view.canvas_rect().center() + view.offset;
    let sample = |point: Vec2| {
        let screen = origin + vec2(point.x, point.y) * view.zoom;
        image
            .get_pixel(screen.x.round() as u32, screen.y.round() as u32)
            .0
    };
    let background = sample(cases[0].0.center());
    let reference = sample(rotate_around(
        cases[1].0.top_center(),
        cases[1].0.center(),
        cases[1].1,
    ));
    assert!(
        reference[0] + 30 < background[0],
        "the translucent stroke must be visible: {reference:?}, {background:?}"
    );
    let similar = |a: [u8; 4], b: [u8; 4]| {
        a.into_iter()
            .zip(b)
            .all(|(left, right)| left.abs_diff(right) <= 3)
    };
    for (mask, (rect, rotation, sides)) in cases.iter().enumerate() {
        for (point, visible) in [
            (rect.top_center(), sides.top),
            (rect.right_center(), sides.right),
            (rect.bottom_center(), sides.bottom),
            (rect.left_center(), sides.left),
        ] {
            let actual = sample(rotate_around(point, rect.center(), *rotation));
            let expected = if visible { reference } else { background };
            assert!(
                similar(actual, expected),
                "mask {mask:04b}, rotation {rotation}, side {point:?}: {actual:?} != {expected:?}"
            );
        }
        if sides.top && sides.left {
            let corner = sample(rotate_around(rect.min, rect.center(), *rotation));
            assert!(
                similar(corner, reference),
                "mask {mask:04b}, rotation {rotation}: corner overdraw {corner:?}"
            );
        }
    }
    assert!(
        similar(sample(Vec2::new(0.0, 380.0)), reference),
        "overlapping sides must be composited once"
    );
    for point in [Vec2::new(-240.0, 357.0), Vec2::new(217.0, 380.0)] {
        assert!(
            similar(sample(point), reference),
            "missing start cap {point:?}"
        );
    }
    for point in [Vec2::new(-240.0, 403.0), Vec2::new(263.0, 380.0)] {
        assert!(
            similar(sample(point), background),
            "collapsed end must not have a square cap {point:?}"
        );
    }
}
