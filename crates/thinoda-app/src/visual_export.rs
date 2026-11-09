use egui::Color32;
use thinoda_io::{ExportOptions, Format, PresentationAspectRatio, VisualExportContext};

pub(crate) fn options_for_gui_export(
    format: Format,
    view_dark: bool,
    aspect_ratio: PresentationAspectRatio,
    allow_symbol_color_loss: bool,
    allow_note_format_loss: bool,
) -> ExportOptions {
    ExportOptions {
        allow_symbol_color_loss,
        allow_note_format_loss,
        dark: view_dark
            && matches!(
                format,
                Format::Png
                    | Format::Jpeg
                    | Format::Svg
                    | Format::DrawIoSvg
                    | Format::DrawIoPng
                    | Format::Pdf
                    | Format::PowerPoint
            ),
        scale: 2.0,
        presentation_aspect_ratio: aspect_ratio,
        ..Default::default()
    }
}

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
    fn gui_export_options_follow_the_resolved_system_theme() {
        let mut app = crate::ThinodaApp::headless();
        app.settings.theme = thinoda_platform::settings::ThemeMode::System;
        let ctx = egui::Context::default();
        let sheet = super::test_helpers::sheet();
        let workbook = thinoda_core::Workbook::new(sheet.clone());

        for theme in [egui::Theme::Light, egui::Theme::Dark, egui::Theme::Light] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    system_theme: Some(theme),
                    ..super::test_helpers::input()
                },
                |ui| app.apply_appearance(ui.ctx()),
            );
            output.textures_delta.clear();
            let view = &app.doc().unwrap().view;
            let dark = theme == egui::Theme::Dark;
            assert_eq!(view.dark, dark);
            for format in [
                Format::Png,
                Format::Jpeg,
                Format::Svg,
                Format::DrawIoSvg,
                Format::DrawIoPng,
                Format::Pdf,
                Format::PowerPoint,
            ] {
                let options = options_for_gui_export(
                    format,
                    view.dark,
                    PresentationAspectRatio::Standard4x3,
                    true,
                    true,
                );
                assert_eq!(options.dark, dark);
                assert_eq!(
                    options.presentation_aspect_ratio,
                    PresentationAspectRatio::Standard4x3
                );
                assert!(options.allow_symbol_color_loss);
                assert!(options.allow_note_format_loss);
                assert_eq!(options.scale, 2.0);
            }
            assert!(
                !options_for_gui_export(
                    Format::Word,
                    view.dark,
                    PresentationAspectRatio::default(),
                    false,
                    false,
                )
                .dark
            );

            let options = options_for_gui_export(
                Format::Svg,
                view.dark,
                PresentationAspectRatio::default(),
                false,
                false,
            );
            let svg = thinoda_io::export_bytes_with_visual_context(
                &workbook,
                Format::Svg,
                &options,
                for_view(view.accent, view.dark),
            )
            .unwrap();
            let svg = String::from_utf8(svg).unwrap();
            let background = thinoda_core::theme::Theme::for_settings(&sheet.theme)
                .background(&sheet.theme, dark);
            let first_rect = svg
                .split("<rect")
                .nth(1)
                .unwrap()
                .split("/>")
                .next()
                .unwrap();
            assert!(first_rect.contains(&format!("fill=\"{}\"", background.to_hex())));
        }
    }

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
