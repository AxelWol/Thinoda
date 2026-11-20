#![cfg(feature = "gpu-tests")]

use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::ThinodaApp;
use thinoda_core::{
    CanvasObject, Color, Rect, ShapeKind, Sheet, SheetRootMode, Vec2, Workbook,
    board::shape_geometry,
};
use thinoda_platform::settings::ThemeMode;

fn selection_menu_shadow(h: &Harness<'_, ThinodaApp>) -> egui::Rect {
    let shadow = h.ctx.global_style().visuals.popup_shadow;
    let margin = f32::from(shadow.blur)
        + f32::from(shadow.spread)
        + f32::from(
            shadow.offset[0]
                .unsigned_abs()
                .max(shadow.offset[1].unsigned_abs()),
        )
        + 2.0;
    h.ctx
        .memory(|memory| memory.area_rect(thinoda_canvas::format_menu::area_id(&h.ctx)))
        .unwrap()
        .expand(margin)
}

#[test]
fn transparent_shape_label_editing_keeps_underlying_content_and_contrast() {
    for (theme, underlay, light_halo) in [
        (ThemeMode::Light, Color::rgb(0, 0, 0), true),
        (ThemeMode::Dark, Color::rgb(255, 255, 255), false),
    ] {
        let mut background = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::ZERO, Vec2::new(360.0, 240.0)),
        );
        background.style.fill = Some(underlay);
        let mut shape = CanvasObject::shape(
            ShapeKind::RoundedRectangle,
            Rect::from_center_size(Vec2::ZERO, Vec2::new(240.0, 150.0)),
        );
        shape.style.stroke = Some(Color::rgb(46, 124, 246));
        shape.style.stroke_width = 2.0;
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        sheet.add_canvas_object(background).unwrap();
        let id = sheet.add_canvas_object(shape).unwrap();
        let mut app = ThinodaApp::headless();
        app.settings.theme = theme;
        app.settings.show_format_panel = false;
        app.new_doc(Workbook::new(sheet));
        app.doc_mut().unwrap().view.select_board_object(id);
        let mut h = Harness::builder()
            .wgpu()
            .with_size(vec2(1200.0, 700.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(6);
        let before = h.render().expect("transparent shape render");
        let view = &h.state().doc().unwrap().view;
        let center = view.canvas_rect().center() + view.offset;
        let sample = ((center.x + 75.0) as u32, (center.y + 30.0) as u32);
        assert_eq!(
            &before.get_pixel(sample.0, sample.1).0[..3],
            &[underlay.r, underlay.g, underlay.b]
        );

        {
            let doc = h.state_mut().doc_mut().unwrap();
            assert!(doc.view.begin_board_label(&doc.doc, id));
        }
        h.run_steps(3);
        h.get_by_label("Board object label").type_text("Readable");
        h.run_steps(3);
        let editing = h.render().expect("transparent shape editor render");
        assert_eq!(
            before.get_pixel(sample.0, sample.1),
            editing.get_pixel(sample.0, sample.1),
            "{theme:?}: label editing obscured the underlying object"
        );
        let mut halo_pixels = 0;
        for y in (center.y as u32 - 15)..=(center.y as u32 + 15) {
            for x in (center.x as u32 - 65)..=(center.x as u32 + 65) {
                let pixel = editing.get_pixel(x, y);
                let contrasting = if light_halo {
                    pixel[0] > 170 && pixel[1] > 170 && pixel[2] > 170
                } else {
                    pixel[0] < 85 && pixel[1] < 85 && pixel[2] < 85
                };
                halo_pixels += usize::from(contrasting);
            }
        }
        assert!(
            halo_pixels > 20,
            "{theme:?}: the edited text needs a contrasting edge over the underlay"
        );
    }
}

#[test]
fn editing_an_empty_rounded_shape_preserves_unobscured_corner_outlines() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for rotation in [0.0, 45.0, 90.0] {
            let rect = Rect::from_center_size(Vec2::ZERO, Vec2::new(240.0, 150.0));
            let mut shape = CanvasObject::shape(ShapeKind::RoundedRectangle, rect);
            shape.rotation = rotation;
            shape.style.stroke = Some(Color::rgb(46, 124, 246));
            shape.style.stroke_width = 2.0;
            let mut sheet = Sheet::new("Board", "");
            sheet.root_mode = SheetRootMode::Hidden;
            let id = sheet.add_canvas_object(shape).unwrap();
            let mut app = ThinodaApp::headless();
            app.settings.theme = theme;
            app.settings.show_format_panel = false;
            app.new_doc(Workbook::new(sheet));
            app.doc_mut().unwrap().view.select_board_object(id);
            let mut h = Harness::builder()
                .wgpu()
                .with_size(vec2(1200.0, 700.0))
                .with_step_dt(1.0 / 60.0)
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(6);
            let before = h.render().expect("selected shape render");
            let before_menu = selection_menu_shadow(&h);
            let view = &h.state().doc().unwrap().view;
            let center = view.canvas_rect().center() + view.offset;
            let zoom = view.zoom;
            let outline = shape_geometry(ShapeKind::RoundedRectangle, rect, rotation).outline;

            {
                let doc = h.state_mut().doc_mut().unwrap();
                assert!(doc.view.begin_board_label(&doc.doc, id));
            }

            h.run_steps(5);
            let editing = h.render().expect("shape label editor render");
            let editing_menu = selection_menu_shadow(&h);
            let mut checked_corners = 0;
            for index in [4, 13, 22, 31] {
                let point = outline[index];
                let x = (center.x + point.x * zoom).round() as i32;
                let y = (center.y + point.y * zoom).round() as i32;
                let sample =
                    egui::Rect::from_center_size(egui::pos2(x as f32, y as f32), vec2(5.0, 5.0));
                // Selection and editing menus legitimately move; compare the canvas outside both shadows.
                if before_menu.intersects(sample) || editing_menu.intersects(sample) {
                    continue;
                }
                checked_corners += 1;
                let mut non_background = 0;
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        let x = u32::try_from(x + dx).unwrap();
                        let y = u32::try_from(y + dy).unwrap();
                        let previous = before.get_pixel(x, y);
                        let current = editing.get_pixel(x, y);
                        if u16::from(previous[2]) > u16::from(previous[0]) + 30 {
                            non_background += 1;
                        }
                        assert_eq!(
                            previous, current,
                            "{theme:?}, {rotation} degrees, corner {index}, ({dx}, {dy})"
                        );
                    }
                }

                assert!(
                    non_background > 0,
                    "{theme:?}, {rotation} degrees, corner {index} did not cover the stroke"
                );
            }
            assert!(
                checked_corners >= 2,
                "{theme:?}, {rotation}: insufficient unobscured corners; menu {before_menu:?} -> {editing_menu:?}, center {center:?}, zoom {zoom}"
            );
        }
    }
}

#[test]
fn editing_guidance_keeps_the_outline_visible_on_compact_canvases() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        // Include the native minimum and an undersized harness. Smaller canvases
        // can legitimately require the nearby menu to overlap the entire shape.
        for size in [vec2(640.0, 360.0), vec2(720.0, 480.0)] {
            let rect = Rect::from_center_size(Vec2::ZERO, Vec2::new(240.0, 150.0));
            let mut shape = CanvasObject::shape(ShapeKind::RoundedRectangle, rect);
            shape.style.stroke = Some(Color::rgb(46, 124, 246));
            shape.style.stroke_width = 2.0;
            let mut sheet = Sheet::new("Board", "");
            sheet.root_mode = SheetRootMode::Hidden;
            let id = sheet.add_canvas_object(shape).unwrap();
            let mut app = ThinodaApp::headless();
            app.settings.theme = theme;
            app.settings.show_format_panel = false;
            app.new_doc(Workbook::new(sheet));
            app.doc_mut().unwrap().view.select_board_object(id);
            let mut h = Harness::builder()
                .wgpu()
                .with_size(size)
                .with_step_dt(1.0 / 60.0)
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(6);
            {
                let view = &mut h.state_mut().doc_mut().unwrap().view;
                view.zoom = 1.0;
                view.offset = egui::Vec2::ZERO;
            }
            h.run_steps(3);
            let before = h.render().expect("compact selected shape render");
            let before_menu = selection_menu_shadow(&h);
            let view = &h.state().doc().unwrap().view;
            let center = view.canvas_rect().center() + view.offset;
            let zoom = view.zoom;
            let canvas = view.canvas_rect();

            {
                let doc = h.state_mut().doc_mut().unwrap();
                assert!(doc.view.begin_board_label(&doc.doc, id));
            }
            h.run_steps(5);
            let editing = h.render().expect("compact shape editor render");
            let editing_menu = selection_menu_shadow(&h);
            let mut stroke_pixels = 0;
            let sides = (-50..=50).flat_map(|y| {
                [-120.0, 120.0].map(|x| (center.x + x * zoom, center.y + y as f32 * zoom))
            });
            let bottom = (-50_i32..=50).filter(|x| x.abs() >= 12).flat_map(|x| {
                [-75.0, 75.0].map(|y| (center.x + x as f32 * zoom, center.y + y * zoom))
            });
            for (x, y) in sides.chain(bottom) {
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        let point = egui::pos2(x.round() + dx as f32, y.round() + dy as f32);
                        if !canvas.contains(point)
                            || before_menu.contains(point)
                            || editing_menu.contains(point)
                            || point.x < 0.0
                            || point.y < 0.0
                            || point.x >= size.x
                            || point.y >= size.y
                        {
                            continue;
                        }
                        let px = point.x as u32;
                        let py = point.y as u32;
                        let previous = before.get_pixel(px, py);
                        if u16::from(previous[2]) > u16::from(previous[0]) + 30 {
                            stroke_pixels += 1;
                            assert_eq!(
                                previous,
                                editing.get_pixel(px, py),
                                "{theme:?}, {size:?}, {canvas:?}, center {center:?}, zoom {zoom}, outline at ({px}, {py})"
                            );
                        }
                    }
                }
            }
            assert!(
                stroke_pixels > 0,
                "{theme:?}, {size:?}: no visible outline sample; menu {before_menu:?} -> {editing_menu:?}, center {center:?}, zoom {zoom}"
            );
        }
    }
}
