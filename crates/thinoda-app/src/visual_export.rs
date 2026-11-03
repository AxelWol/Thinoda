use egui::Color32;
use thinoda_io::VisualExportContext;

pub(crate) fn for_view(accent: Color32, dark: bool) -> VisualExportContext {
    let [r, g, b, a] = thinoda_canvas::automatic_board_stroke(accent, dark).to_srgba_unmultiplied();
    VisualExportContext::default()
        .with_automatic_board_stroke(thinoda_core::Color::rgba(r, g, b, a))
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;
    use thinoda_core::{
        CanvasConnector, CanvasObject, Endpoint, PresentationSource, Rect, Sheet, SheetRootMode,
        Vec2,
    };

    pub(crate) fn sheet() -> Sheet {
        let mut sheet = Sheet::new("Preview", "");
        sheet.root_mode = SheetRootMode::Hidden;
        sheet.add_floating("Card", Vec2::new(100.0, 130.0));
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::new(20.0, 50.0)),
            Endpoint::Point(Vec2::new(220.0, 50.0)),
        );
        connector.width = 8.0;
        connector.arrow_end = false;
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        sheet
            .add_presentation_slide(
                "View",
                PresentationSource::Viewport,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(260.0, 200.0)),
            )
            .unwrap();
        sheet
    }

    pub(crate) fn context(accent: Color32, dark: bool) -> egui::Context {
        let ctx = egui::Context::default();
        set_accent(&ctx, accent, dark);
        ctx
    }

    pub(crate) fn set_accent(ctx: &egui::Context, accent: Color32, dark: bool) {
        thinoda_ui::theme::install(
            ctx,
            if dark {
                thinoda_ui::theme::ThemeMode::Dark
            } else {
                thinoda_ui::theme::ThemeMode::Light
            },
            accent,
        );
    }

    pub(crate) fn input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500.0, 400.0),
            )),
            ..Default::default()
        }
    }

    pub(crate) fn assert_stroke(output: &egui::FullOutput, color: Color32) {
        fn contains(shape: &egui::Shape, color: Color32) -> bool {
            match shape {
                egui::Shape::Path(path) => {
                    path.stroke.color == egui::epaint::ColorMode::Solid(color)
                }
                egui::Shape::LineSegment { stroke, .. } => stroke.color == color,
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| contains(shape, color)),
                _ => false,
            }
        }
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| contains(&shape.shape, color)),
            "preview did not paint the automatic board stroke {color:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thinoda_core::Color;

    #[test]
    fn resolves_the_same_accent_and_transparent_fallback_as_the_canvas() {
        let accent = Color32::from_rgba_unmultiplied(35, 155, 210, 170);
        let [r, g, b, a] = accent.to_srgba_unmultiplied();
        let expected =
            VisualExportContext::default().with_automatic_board_stroke(Color::rgba(r, g, b, a));
        assert_eq!(for_view(accent, false), expected);
        assert_eq!(for_view(accent, true), expected);

        for (dark, gray) in [(false, 65), (true, 225)] {
            let expected = VisualExportContext::default()
                .with_automatic_board_stroke(Color::rgb(gray, gray, gray));
            assert_eq!(for_view(Color32::TRANSPARENT, dark), expected);

            let workbook = thinoda_core::Workbook::new(super::test_helpers::sheet());
            let svg = thinoda_io::export_bytes_with_visual_context(
                &workbook,
                thinoda_io::Format::Svg,
                &thinoda_io::ExportOptions {
                    dark,
                    ..Default::default()
                },
                for_view(Color32::TRANSPARENT, dark),
            )
            .unwrap();
            let svg = String::from_utf8(svg).unwrap();
            assert!(svg.contains(&format!(
                "stroke=\"{}\"",
                Color::rgb(gray, gray, gray).to_hex()
            )));
        }
    }
}
