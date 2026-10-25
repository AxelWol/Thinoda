//! Deterministic slide generation and exports using saved presentation frames.

use image::ImageFormat;
use thinoda_core::{Presentation, PresentationSlide, PresentationSource, Rect, Sheet, SlideId};

use crate::{
    ExportOptions, IoError,
    render::{self, PdfImagePage},
};

pub const MAX_PRESENTATION_IMAGE_SIDE: u32 = 16_384;
pub const MAX_PRESENTATION_IMAGE_PIXELS: u64 = 16_000_000;
pub const MAX_PRESENTATION_TOTAL_PIXELS: u64 = 32_000_000;
pub const MAX_PRESENTATION_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;

const MAX_PRESENTATION_PAGE_POINTS: f32 = 20_000.0;

/// Generates one slide for each central-topic branch, followed by one per Zone.
/// The returned slide order and frames can be persisted with `Document::replace_presentation`.
pub fn generate_presentation(
    sheet: &Sheet,
    options: &ExportOptions,
) -> Result<Presentation, IoError> {
    sheet.validate().map_err(IoError::Invalid)?;
    if sheet.children(sheet.root).len() + sheet.zones.len() > thinoda_core::MAX_PRESENTATION_SLIDES
    {
        return Err(IoError::PresentationLimit("slide count"));
    }
    let branches = sheet.children(sheet.root);
    let frames = render::branch_frames(sheet, branches, options.dark)?;
    let mut slides = Vec::new();
    for (root, frame) in branches.iter().zip(frames) {
        let title = sheet
            .topic(*root)
            .ok_or_else(|| IoError::Invalid(format!("missing branch topic {root}")))?
            .title
            .clone();
        slides.push(PresentationSlide {
            id: SlideId(slides.len() as u64 + 1),
            title,
            source: PresentationSource::Branch(*root),
            frame,
        });
    }
    for zone in &sheet.zones {
        slides.push(PresentationSlide {
            id: SlideId(slides.len() as u64 + 1),
            title: zone.name.clone(),
            source: PresentationSource::Zone(zone.id),
            frame: zone.frame,
        });
    }
    if slides.len() > thinoda_core::MAX_PRESENTATION_SLIDES {
        return Err(IoError::PresentationLimit("slide count"));
    }
    let presentation = Presentation { slides };
    presentation.validate(sheet).map_err(IoError::Invalid)?;
    Ok(presentation)
}

/// Renders a saved slide with all sheet content clipped to its saved frame.
/// Relationships are drawn from the full sheet before the SVG viewport clips them.
pub fn render_presentation_slide_svg(
    sheet: &Sheet,
    slide_index: usize,
    options: &ExportOptions,
) -> Result<String, IoError> {
    sheet.validate().map_err(IoError::Invalid)?;
    let slide =
        sheet.presentation.slides.get(slide_index).ok_or_else(|| {
            IoError::Invalid(format!("presentation slide {slide_index} not found"))
        })?;
    render::svg_with_frame(sheet, options, slide.frame)
}

/// Renders one saved slide frame to a bounded PNG for preview textures.
pub fn render_presentation_slide_png(
    sheet: &Sheet,
    slide_index: usize,
    options: &ExportOptions,
) -> Result<Vec<u8>, IoError> {
    validate_scale(options.scale)?;
    let slide =
        sheet.presentation.slides.get(slide_index).ok_or_else(|| {
            IoError::Invalid(format!("presentation slide {slide_index} not found"))
        })?;
    let mut total_pixels = 0;
    image_dimensions(slide.frame, options.scale, &mut total_pixels)?;
    let svg = render_presentation_slide_svg(sheet, slide_index, options)?;
    render::raster_svg(&svg, options.scale, ImageFormat::Png)
}

/// Exports the saved slide frames, in presentation order, as `slides/slide-NNN.png`.
pub fn export_presentation_png_zip(
    sheet: &Sheet,
    options: &ExportOptions,
) -> Result<Vec<u8>, IoError> {
    sheet.validate().map_err(IoError::Invalid)?;
    validate_slides(sheet)?;
    validate_scale(options.scale)?;

    let mut total_pixels = 0;
    let mut rendered = Vec::with_capacity(sheet.presentation.slides.len());
    for (index, slide) in sheet.presentation.slides.iter().enumerate() {
        image_dimensions(slide.frame, options.scale, &mut total_pixels)?;
        let svg = render::svg_with_frame(sheet, options, slide.frame)?;
        let png = render::raster_svg(&svg, options.scale, ImageFormat::Png)?;
        rendered.push((format!("slides/slide-{:03}.png", index + 1), png));
    }
    let entries: Vec<_> = rendered
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    let archive = crate::formats::archive(&entries)?;
    if archive.len() > MAX_PRESENTATION_ARCHIVE_BYTES {
        return Err(IoError::PresentationLimit("archive size"));
    }
    Ok(archive)
}

/// Exports the saved slide frames, in presentation order, as a multi-page PDF.
pub fn export_presentation_pdf(sheet: &Sheet, options: &ExportOptions) -> Result<Vec<u8>, IoError> {
    sheet.validate().map_err(IoError::Invalid)?;
    validate_slides(sheet)?;
    validate_scale(options.scale)?;

    let mut total_pixels = 0;
    let mut images = Vec::with_capacity(sheet.presentation.slides.len());
    for slide in &sheet.presentation.slides {
        let (width, height) = image_dimensions(slide.frame, options.scale, &mut total_pixels)?;
        let page_width = slide.frame.size.x * 0.75;
        let page_height = slide.frame.size.y * 0.75;
        if page_width > MAX_PRESENTATION_PAGE_POINTS || page_height > MAX_PRESENTATION_PAGE_POINTS {
            return Err(IoError::PresentationLimit("PDF page dimensions"));
        }
        let mut opaque_options = options.clone();
        opaque_options.transparent = false;
        let svg = render::svg_with_frame(sheet, &opaque_options, slide.frame)?;
        let jpg = render::raster_svg(&svg, options.scale, ImageFormat::Jpeg)?;
        images.push((jpg, width, height, page_width, page_height));
    }
    let pages: Vec<_> = images
        .iter()
        .map(
            |(jpg, width, height, page_width, page_height)| PdfImagePage {
                jpg,
                width: *width,
                height: *height,
                page_width: *page_width,
                page_height: *page_height,
            },
        )
        .collect();
    let pdf = render::image_pdf_pages(&pages);
    if pdf.len() > MAX_PRESENTATION_ARCHIVE_BYTES {
        return Err(IoError::PresentationLimit("PDF size"));
    }
    Ok(pdf)
}

fn validate_slides(sheet: &Sheet) -> Result<(), IoError> {
    if sheet.presentation.slides.is_empty() {
        return Err(IoError::Invalid("presentation has no slides".into()));
    }
    Ok(())
}

fn validate_scale(scale: f32) -> Result<(), IoError> {
    if !scale.is_finite() || !(0.1..=8.0).contains(&scale) {
        return Err(IoError::Invalid(
            "presentation export scale must be finite and within 0.1..=8".into(),
        ));
    }
    Ok(())
}

fn image_dimensions(
    frame: Rect,
    scale: f32,
    total_pixels: &mut u64,
) -> Result<(u32, u32), IoError> {
    let width = (frame.size.x * scale).ceil();
    let height = (frame.size.y * scale).ceil();
    if !width.is_finite()
        || !height.is_finite()
        || width < 1.0
        || height < 1.0
        || width > MAX_PRESENTATION_IMAGE_SIDE as f32
        || height > MAX_PRESENTATION_IMAGE_SIDE as f32
    {
        return Err(IoError::PresentationLimit("image side"));
    }
    let width = width as u32;
    let height = height as u32;
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_PRESENTATION_IMAGE_PIXELS {
        return Err(IoError::PresentationLimit("per-slide pixel count"));
    }
    *total_pixels = total_pixels
        .checked_add(pixels)
        .ok_or(IoError::PresentationLimit("total pixel count"))?;
    if *total_pixels > MAX_PRESENTATION_TOTAL_PIXELS {
        return Err(IoError::PresentationLimit("total pixel count"));
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use std::io::Cursor;
    use thinoda_core::{RelationshipId, TopicId, Vec2};
    use zip::ZipArchive;

    fn frame(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, height))
    }

    fn sample_sheet() -> Sheet {
        let mut sheet = Sheet::new("Map", "Central");
        let branch = sheet.add_child(sheet.root, None, "Branch");
        let child = sheet.add_child(branch, None, "Child");
        let floating = sheet.add_floating("Floating", Vec2::new(280.0, 120.0));
        sheet.relationships.push(thinoda_core::Relationship {
            id: RelationshipId(900),
            from: child,
            to: floating,
            title: "Across frame".into(),
            color: None,
            control_points: None,
        });
        let zone = sheet
            .create_zone("Ideas", vec![floating], frame(180.0, 40.0, 180.0, 160.0))
            .unwrap();
        sheet
            .add_presentation_slide(
                "Branch frame",
                PresentationSource::Branch(branch),
                frame(-20.0, -40.0, 180.0, 140.0),
            )
            .unwrap();
        sheet
            .add_presentation_slide(
                "Zone frame",
                PresentationSource::Zone(zone),
                frame(180.0, 40.0, 180.0, 160.0),
            )
            .unwrap();
        sheet.validate().unwrap();
        sheet
    }

    #[test]
    fn generation_orders_branches_then_zones_and_uses_zone_frames() {
        let mut sheet = Sheet::new("Map", "Central");
        let first = sheet.add_child(sheet.root, None, "First");
        sheet.add_child(first, None, "Nested");
        sheet.add_child(sheet.root, None, "Second");
        let floating = sheet.add_floating("Floating", Vec2::new(200.0, 120.0));
        sheet
            .create_zone("Area", vec![floating], frame(100.0, 20.0, 160.0, 140.0))
            .unwrap();
        let presentation = generate_presentation(&sheet, &ExportOptions::default()).unwrap();
        assert_eq!(presentation.slides.len(), 3);
        assert_eq!(
            presentation.slides[0].source,
            PresentationSource::Branch(first)
        );
        assert_eq!(presentation.slides[1].title, "Second");
        assert_eq!(
            presentation.slides[2].source,
            PresentationSource::Zone(sheet.zones[0].id)
        );
        assert_eq!(presentation.slides[2].frame, sheet.zones[0].frame);
        presentation.validate(&sheet).unwrap();
    }

    #[test]
    fn svg_preview_uses_saved_frame_and_keeps_cross_frame_relationships_for_clipping() {
        let sheet = sample_sheet();
        let svg = render_presentation_slide_svg(&sheet, 0, &ExportOptions::default()).unwrap();
        assert!(svg.contains("viewBox=\"-20.00 -40.00 180.00 140.00\""));
        assert!(svg.contains("Across frame"));
        let zone_svg = render_presentation_slide_svg(&sheet, 1, &ExportOptions::default()).unwrap();
        assert!(zone_svg.contains("viewBox=\"180.00 40.00 180.00 160.00\""));
        assert!(zone_svg.contains(">Ideas</text>"));
    }

    #[test]
    fn single_slide_png_uses_the_same_saved_frame_and_bounds() {
        let sheet = sample_sheet();
        let png = render_presentation_slide_png(&sheet, 1, &ExportOptions::default()).unwrap();
        assert_eq!(
            image::load_from_memory(&png).unwrap().dimensions(),
            (180, 160)
        );

        let mut oversized = sheet.clone();
        oversized.presentation.slides[1].frame = frame(0.0, 0.0, 3_000.0, 2_000.0);
        let options = ExportOptions {
            scale: 8.0,
            ..ExportOptions::default()
        };
        assert!(matches!(
            render_presentation_slide_png(&oversized, 1, &options),
            Err(IoError::PresentationLimit("image side"))
        ));
    }

    #[test]
    fn pdf_and_ordered_png_zip_share_saved_frames_without_mutation() {
        let sheet = sample_sheet();
        let before = sheet.clone();
        let zip_bytes = export_presentation_png_zip(&sheet, &ExportOptions::default()).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(zip_bytes)).unwrap();
        assert_eq!(archive.len(), 2);
        let mut first = archive.by_index(0).unwrap();
        assert_eq!(first.name().unwrap(), "slides/slide-001.png");
        let mut first_png = Vec::new();
        std::io::Read::read_to_end(&mut first, &mut first_png).unwrap();
        let first_size = image::load_from_memory(&first_png).unwrap().dimensions();
        assert_eq!(first_size, (180, 140));
        drop(first);
        let mut second = archive.by_index(1).unwrap();
        assert_eq!(second.name().unwrap(), "slides/slide-002.png");
        let mut second_png = Vec::new();
        std::io::Read::read_to_end(&mut second, &mut second_png).unwrap();
        assert_eq!(
            image::load_from_memory(&second_png).unwrap().dimensions(),
            (180, 160)
        );

        let pdf = export_presentation_pdf(&sheet, &ExportOptions::default()).unwrap();
        let pdf = String::from_utf8_lossy(&pdf);
        assert!(pdf.contains("/Count 2"));
        assert_eq!(pdf.matches("/Type /Page ").count(), 2);
        assert!(pdf.contains("/MediaBox [0 0 135.000 105.000]"));
        assert!(pdf.contains("/MediaBox [0 0 135.000 120.000]"));
        assert_eq!(sheet, before);
    }

    #[test]
    fn native_document_roundtrip_persists_zone_and_slide_frames() {
        let sheet = sample_sheet();
        let book = thinoda_core::Workbook::new(sheet.clone());
        let bytes =
            crate::export_bytes(&book, crate::Format::Thinoda, &ExportOptions::default()).unwrap();
        let restored = crate::import_bytes(crate::Format::Thinoda, &bytes).unwrap();
        assert_eq!(restored.sheets[0], sheet);
    }

    #[test]
    fn export_limits_reject_oversized_frames_and_empty_presentations() {
        let mut sheet = sample_sheet();
        sheet.presentation.slides.clear();
        assert!(export_presentation_pdf(&sheet, &ExportOptions::default()).is_err());
        assert!(export_presentation_png_zip(&sheet, &ExportOptions::default()).is_err());

        sheet.presentation.slides.push(PresentationSlide {
            id: SlideId(42),
            title: "Huge".into(),
            source: PresentationSource::Branch(
                sheet
                    .children(sheet.root)
                    .first()
                    .copied()
                    .unwrap_or(TopicId(0)),
            ),
            frame: frame(0.0, 0.0, 1_000_000.0, 1_000_000.0),
        });
        assert!(export_presentation_pdf(&sheet, &ExportOptions::default()).is_err());
        assert!(export_presentation_png_zip(&sheet, &ExportOptions::default()).is_err());
    }
}
