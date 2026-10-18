//! Paper-sized printing using the same complete-sheet SVG as image exports.
//! No viewport, zoom, selection or editor chrome is captured.

use std::io::Cursor;

use image::{ImageFormat, RgbImage};
use thinoda_core::Workbook;

use crate::{ExportOptions, IoError, render};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Paper {
    #[default]
    A4,
    A3,
    Letter,
    Legal,
}

impl Paper {
    pub const ALL: [Self; 4] = [Self::A4, Self::A3, Self::Letter, Self::Legal];

    pub fn label(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::A3 => "A3",
            Self::Letter => "Letter",
            Self::Legal => "Legal",
        }
    }

    pub fn size_mm(self) -> [f32; 2] {
        match self {
            Self::A4 => [210.0, 297.0],
            Self::A3 => [297.0, 420.0],
            Self::Letter => [215.9, 279.4],
            Self::Legal => [215.9, 355.6],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrintSettings {
    pub paper: Paper,
    pub landscape: bool,
    pub margin_mm: f32,
    pub dark: bool,
    pub background: bool,
    pub grayscale: bool,
    pub dpi: u32,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            paper: Paper::A4,
            landscape: true,
            margin_mm: 12.0,
            dark: false,
            background: false,
            grayscale: false,
            dpi: 150,
        }
    }
}

impl PrintSettings {
    pub fn size_mm(self) -> [f32; 2] {
        let [w, h] = self.paper.size_mm();
        if self.landscape { [h, w] } else { [w, h] }
    }

    fn validate(self) -> Result<(), IoError> {
        if !self.margin_mm.is_finite()
            || !(0.0..=40.0).contains(&self.margin_mm)
            || !matches!(self.dpi, 150 | 300)
        {
            return Err(IoError::Invalid("invalid print settings".into()));
        }
        Ok(())
    }

    /// Center and fit without cropping or distorting. Coordinates are millimeters.
    pub fn placement(self, source: [f32; 2]) -> Result<[f32; 4], IoError> {
        self.validate()?;
        if source.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            return Err(IoError::Invalid("invalid print source dimensions".into()));
        }
        let [w, h] = self.size_mm();
        let inner = [w - 2.0 * self.margin_mm, h - 2.0 * self.margin_mm];
        let scale = (inner[0] / source[0]).min(inner[1] / source[1]);
        let size = [source[0] * scale, source[1] * scale];
        Ok([(w - size[0]) / 2.0, (h - size[1]) / 2.0, size[0], size[1]])
    }
}

/// The preview, native printer page and PDF derive from the same page pixels.
/// Native output is lossless; PDF embeds a high-quality JPEG, like existing export.
pub struct PrintDocument {
    pub settings: PrintSettings,
    pub image: RgbImage,
}

impl PrintDocument {
    pub fn png(&self) -> Result<Vec<u8>, IoError> {
        let mut out = Cursor::new(Vec::new());
        self.image
            .write_to(&mut out, ImageFormat::Png)
            .map_err(|e| IoError::Invalid(e.to_string()))?;
        Ok(out.into_inner())
    }

    pub fn pdf(&self) -> Result<Vec<u8>, IoError> {
        let mut out = Cursor::new(Vec::new());
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95)
            .encode_image(&self.image)
            .map_err(|e| IoError::Invalid(e.to_string()))?;
        let [w, h] = self.settings.size_mm();
        Ok(render::image_pdf(
            &out.into_inner(),
            self.image.width(),
            self.image.height(),
            w * 72.0 / 25.4,
            h * 72.0 / 25.4,
        ))
    }
}

pub fn render_print(
    workbook: &Workbook,
    sheet: usize,
    settings: PrintSettings,
) -> Result<PrintDocument, IoError> {
    settings.validate()?;
    if sheet >= workbook.sheets.len() {
        return Err(IoError::Invalid("print sheet does not exist".into()));
    }
    let source = render::svg(
        workbook,
        &ExportOptions {
            sheet: Some(sheet),
            dark: settings.dark,
            transparent: !(settings.background || settings.dark),
            padding: 12.0,
            ..Default::default()
        },
    )?;
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&source, &options)
        .map_err(|e| IoError::Invalid(format!("SVG parse: {e}")))?;
    let source_size = tree.size();
    let [x, y, w, _] = settings.placement([source_size.width(), source_size.height()])?;
    let [page_w, page_h] = settings.size_mm();
    let pixels_per_mm = settings.dpi as f32 / 25.4;
    let width = (page_w * pixels_per_mm).round() as u32;
    let height = (page_h * pixels_per_mm).round() as u32;
    // Largest supported page, A3 at 300 dpi, is under 18 million pixels.
    if u64::from(width) * u64::from(height) > 18_000_000 {
        return Err(IoError::Invalid("print image exceeds limits".into()));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| IoError::Invalid("print image allocation failed".into()))?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    let scale = w * pixels_per_mm / source_size.width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_row(
            scale,
            0.0,
            0.0,
            scale,
            x * pixels_per_mm,
            y * pixels_per_mm,
        ),
        &mut pixmap.as_mut(),
    );
    // Render onto white before removing alpha, avoiding premultiplication artifacts.
    let rgba = image::RgbaImage::from_raw(width, height, pixmap.take())
        .ok_or_else(|| IoError::Invalid("invalid print image".into()))?;
    let mut image = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
    if settings.grayscale {
        image = image::DynamicImage::ImageRgb8(image).grayscale().to_rgb8();
    }
    Ok(PrintDocument { settings, image })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_paper_orientation_fits_and_centers_extreme_maps() {
        for paper in Paper::ALL {
            for landscape in [false, true] {
                for margin_mm in [0.0, 12.0, 40.0] {
                    let settings = PrintSettings {
                        paper,
                        landscape,
                        margin_mm,
                        ..Default::default()
                    };
                    let [pw, ph] = settings.size_mm();
                    for source in [[1.0, 1.0], [100_000.0, 10.0], [10.0, 100_000.0]] {
                        let [x, y, w, h] = settings.placement(source).unwrap();
                        assert!(x >= margin_mm - 0.001 && y >= margin_mm - 0.001);
                        assert!((2.0 * x + w - pw).abs() < 0.001);
                        assert!((2.0 * y + h - ph).abs() < 0.001);
                        assert!((w / h / (source[0] / source[1]) - 1.0).abs() < 0.001);
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_invalid_settings_dimensions_and_sheet() {
        for margin_mm in [f32::NAN, f32::INFINITY, -1.0, 41.0] {
            assert!(
                PrintSettings {
                    margin_mm,
                    ..Default::default()
                }
                .placement([1.0, 1.0])
                .is_err()
            );
        }
        assert!(
            PrintSettings {
                dpi: u32::MAX,
                ..Default::default()
            }
            .placement([1.0, 1.0])
            .is_err()
        );
        for source in [[0.0, 1.0], [-1.0, 1.0], [f32::NAN, 1.0]] {
            assert!(PrintSettings::default().placement(source).is_err());
        }
        assert!(render_print(&Workbook::default(), usize::MAX, PrintSettings::default()).is_err());
    }

    #[test]
    fn output_has_physical_page_size_white_margins_and_grayscale() {
        let book = Workbook::new(thinoda_core::Sheet::new("Sheet", "Printed map"));
        let settings = PrintSettings {
            grayscale: true,
            ..Default::default()
        };
        let document = render_print(&book, 0, settings).unwrap();
        assert_eq!(document.image.dimensions(), (1754, 1240));
        assert_eq!(document.image.get_pixel(0, 0).0, [255, 255, 255]);
        assert!(document.image.pixels().any(|p| p.0[0] < 200));
        assert!(
            document
                .image
                .pixels()
                .all(|p| p.0[0] == p.0[1] && p.0[1] == p.0[2])
        );
        let png = document.png().unwrap();
        assert_eq!(
            image::load_from_memory(&png).unwrap().to_rgb8(),
            document.image
        );
        let pdf = document.pdf().unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/MediaBox [0 0 841.890 595.276]"));
        assert!(text.contains("/Count 1"));
        assert!(text.ends_with("%%EOF"));
    }

    #[test]
    fn offscreen_floating_topics_are_not_cropped_and_only_selected_sheet_is_printed() {
        use thinoda_core::{Color, Vec2};
        let mut sheet = thinoda_core::Sheet::new("Print this sheet", "");
        sheet.topics.get_mut(&sheet.root).unwrap().style.fill = Some(Color::rgb(255, 0, 0));
        let far = sheet.add_floating("", Vec2::new(10_000.0, 0.0));
        sheet.topics.get_mut(&far).unwrap().style.fill = Some(Color::rgb(0, 0, 255));
        let mut book = Workbook::new(thinoda_core::Sheet::new("Do not print", "Wrong sheet"));
        book.sheets.push(sheet);
        let page = render_print(&book, 1, PrintSettings::default()).unwrap();
        let is_red = |p: &image::Rgb<u8>| p.0[0] > 180 && p.0[1] < 80 && p.0[2] < 80;
        let is_blue = |p: &image::Rgb<u8>| p.0[2] > 180 && p.0[0] < 80 && p.0[1] < 80;
        let red = page
            .image
            .enumerate_pixels()
            .filter(|(_, _, p)| is_red(p))
            .map(|(x, _, _)| x)
            .min()
            .unwrap();
        let blue = page
            .image
            .enumerate_pixels()
            .filter(|(_, _, p)| is_blue(p))
            .map(|(x, _, _)| x)
            .max()
            .unwrap();
        assert!(blue - red > page.image.width() / 2);
        book.sheets[0].topics.values_mut().next().unwrap().title = "Unrelated changes".into();
        assert_eq!(
            page.image,
            render_print(&book, 1, PrintSettings::default())
                .unwrap()
                .image
        );
    }

    #[test]
    fn high_resolution_portrait_and_dark_background_are_supported() {
        let book = Workbook::default();
        let settings = PrintSettings {
            paper: Paper::A3,
            landscape: false,
            dpi: 300,
            dark: true,
            ..Default::default()
        };
        let document = render_print(&book, 0, settings).unwrap();
        assert_eq!(document.image.dimensions(), (3508, 4961));
        assert_eq!(document.image.get_pixel(0, 0).0, [255; 3]);
        assert!(
            document
                .image
                .pixels()
                .any(|p| p.0.iter().all(|c| *c < 100))
        );
    }
}
