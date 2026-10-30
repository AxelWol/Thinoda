//! Deterministic slide generation and live, source-aware presentation exports.

use image::ImageFormat;
use thinoda_core::{
    Presentation, PresentationScope, PresentationSlide, PresentationSource, Rect,
    ResolvedPresentationSlide, Sheet, SlideId, resolve_presentation_slide,
};

use crate::{ExportOptions, IoError, render};

const PPTX_IMAGE_WIDTH: f32 = 1280.0;
const EMU_PER_INCH: u64 = 914_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresentationAspectRatio {
    Standard4x3,
    #[default]
    Widescreen16x9,
}

impl PresentationAspectRatio {
    fn dimensions(self) -> (f32, f32, u64, u64, &'static str) {
        match self {
            Self::Standard4x3 => (4.0 / 3.0, 960.0, 10 * EMU_PER_INCH, 6_858_000, "screen4x3"),
            Self::Widescreen16x9 => (
                16.0 / 9.0,
                720.0,
                13 * EMU_PER_INCH + EMU_PER_INCH / 3,
                6_858_000,
                "screen16x9",
            ),
        }
    }
}

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
            scope: PresentationScope::Source,
            auto_frame: true,
            hidden_topics: Vec::new(),
            max_depth: None,
        });
    }
    for zone in &sheet.zones {
        slides.push(PresentationSlide {
            id: SlideId(slides.len() as u64 + 1),
            title: zone.name.clone(),
            source: PresentationSource::Zone(zone.id),
            frame: zone.frame,
            scope: PresentationScope::Source,
            auto_frame: true,
            hidden_topics: Vec::new(),
            max_depth: None,
        });
    }
    if slides.len() > thinoda_core::MAX_PRESENTATION_SLIDES {
        return Err(IoError::PresentationLimit("slide count"));
    }
    let presentation = Presentation { slides };
    presentation.validate(sheet).map_err(IoError::Invalid)?;
    Ok(presentation)
}

/// Renders live slide content using the shared core resolver and original map geometry.
/// Legacy contextual slides retain spatial clipping; source slides exclude unrelated content.
pub fn render_presentation_slide_svg(
    sheet: &Sheet,
    slide_index: usize,
    options: &ExportOptions,
) -> Result<String, IoError> {
    sheet.validate().map_err(IoError::Invalid)?;
    let resolved = resolve_slide(sheet, slide_index, options)?;
    render::svg_resolved(sheet, options, &resolved)
}

fn resolve_slide(
    sheet: &Sheet,
    slide_index: usize,
    options: &ExportOptions,
) -> Result<ResolvedPresentationSlide, IoError> {
    let slide =
        sheet.presentation.slides.get(slide_index).ok_or_else(|| {
            IoError::Invalid(format!("presentation slide {slide_index} not found"))
        })?;
    let layout = render::presentation_layout(sheet, options.dark)?;
    resolve_presentation_slide(sheet, slide, &layout).map_err(IoError::Invalid)
}

/// Renders one resolved slide to a bounded PNG for preview textures.
pub fn render_presentation_slide_png(
    sheet: &Sheet,
    slide_index: usize,
    options: &ExportOptions,
) -> Result<Vec<u8>, IoError> {
    validate_scale(options.scale)?;
    sheet.validate().map_err(IoError::Invalid)?;
    let resolved = resolve_slide(sheet, slide_index, options)?;
    let mut total_pixels = 0;
    image_dimensions(resolved.frame, options.scale, &mut total_pixels)?;
    let svg = render::svg_resolved(sheet, options, &resolved)?;
    render::raster_svg(&svg, options.scale, ImageFormat::Png)
}

/// Exports resolved slides, in presentation order, as `slides/slide-NNN.png`.
pub fn export_presentation_png_zip(
    sheet: &Sheet,
    options: &ExportOptions,
) -> Result<Vec<u8>, IoError> {
    sheet.validate().map_err(IoError::Invalid)?;
    validate_slides(sheet)?;
    validate_scale(options.scale)?;

    let mut total_pixels = 0;
    let mut rendered = Vec::with_capacity(sheet.presentation.slides.len());
    for index in 0..sheet.presentation.slides.len() {
        let resolved = resolve_slide(sheet, index, options)?;
        image_dimensions(resolved.frame, options.scale, &mut total_pixels)?;
        let svg = render::svg_resolved(sheet, options, &resolved)?;
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

/// Exports live slide content as editable native PowerPoint shapes, text and connectors.
///
/// Each saved frame is fitted to the selected deck aspect ratio by extending its
/// viewport, so its original content is neither stretched nor cropped.
pub fn export_presentation_pptx(
    sheet: &Sheet,
    options: &ExportOptions,
) -> Result<Vec<u8>, IoError> {
    crate::reject_equation_notes(sheet)?;
    sheet.validate().map_err(IoError::Invalid)?;
    validate_slides(sheet)?;
    validate_scale(options.scale)?;

    let ratio = options.presentation_aspect_ratio;
    let (target_ratio, target_height, width_emu, height_emu, size_type) = ratio.dimensions();
    let target_width = PPTX_IMAGE_WIDTH.min(target_height * target_ratio);
    let mut total_pixels = 0;
    let mut native_slides = Vec::with_capacity(sheet.presentation.slides.len());
    let mut native_bytes = 0usize;

    for (index, slide) in sheet.presentation.slides.iter().enumerate() {
        let mut resolved = resolve_slide(sheet, index, options)?;
        resolved.frame = fit_frame(resolved.frame, target_ratio)?;
        // Keep the existing side, area and aggregate complexity guards even
        // though this path no longer allocates a slide-sized bitmap.
        let scale = ((target_width - 0.1) / resolved.frame.size.x).clamp(0.1, 8.0);
        image_dimensions(resolved.frame, scale, &mut total_pixels)?;
        let native =
            render::editable_slide(sheet, &resolved, options, index + 1, width_emu, height_emu)?;
        native_bytes = native_bytes
            .saturating_add(native.shapes.len())
            .saturating_add(
                native
                    .media
                    .iter()
                    .map(|(_, data)| data.len())
                    .sum::<usize>(),
            );
        if native_bytes > MAX_PRESENTATION_ARCHIVE_BYTES {
            return Err(IoError::PresentationLimit("archive size"));
        }
        native_slides.push((index + 1, slide.title.clone(), native));
    }

    let mut entries: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".into(),
            content_types(sheet.presentation.slides.len()).into_bytes(),
        ),
        (
            "_rels/.rels".into(),
            root_relationships().as_bytes().to_vec(),
        ),
        (
            "ppt/presentation.xml".into(),
            presentation_xml(
                sheet.presentation.slides.len(),
                width_emu,
                height_emu,
                size_type,
            )
            .into_bytes(),
        ),
        (
            "ppt/_rels/presentation.xml.rels".into(),
            presentation_relationships(sheet.presentation.slides.len()).into_bytes(),
        ),
        (
            "ppt/slideMasters/slideMaster1.xml".into(),
            slide_master_xml().as_bytes().to_vec(),
        ),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".into(),
            slide_master_relationships().as_bytes().to_vec(),
        ),
        (
            "ppt/slideLayouts/slideLayout1.xml".into(),
            slide_layout_xml().as_bytes().to_vec(),
        ),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(),
            slide_layout_relationships().as_bytes().to_vec(),
        ),
        (
            "ppt/theme/theme1.xml".into(),
            theme_xml().as_bytes().to_vec(),
        ),
    ];
    for (index, title, native) in native_slides {
        let slide_number = format!("{index}");
        let slide_path = format!("ppt/slides/slide{slide_number}.xml");
        let rels_path = format!("ppt/slides/_rels/slide{slide_number}.xml.rels");
        entries.push((
            slide_path,
            slide_xml(&title, &native.shapes, &native.background).into_bytes(),
        ));
        entries.push((rels_path, native.relationships.into_bytes()));
        entries.extend(native.media);
    }

    let archive_entries: Vec<_> = entries
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    let archive = crate::formats::archive(&archive_entries)?;
    if archive.len() > MAX_PRESENTATION_ARCHIVE_BYTES {
        return Err(IoError::PresentationLimit("archive size"));
    }
    Ok(archive)
}

/// Renders one saved slide using the same fitted frame and background as PowerPoint export.
pub fn render_presentation_pptx_slide_png(
    sheet: &Sheet,
    slide_index: usize,
    options: &ExportOptions,
) -> Result<Vec<u8>, IoError> {
    sheet.validate().map_err(IoError::Invalid)?;
    let (target_ratio, target_height, ..) = options.presentation_aspect_ratio.dimensions();
    let target_width = (PPTX_IMAGE_WIDTH / 2.0).min(target_height * target_ratio / 2.0);
    let mut total_pixels = 0;
    render_pptx_slide(
        sheet,
        slide_index,
        options,
        target_ratio,
        target_width,
        &mut total_pixels,
    )
}

fn render_pptx_slide(
    sheet: &Sheet,
    slide_index: usize,
    options: &ExportOptions,
    target_ratio: f32,
    target_width: f32,
    total_pixels: &mut u64,
) -> Result<Vec<u8>, IoError> {
    let mut resolved = resolve_slide(sheet, slide_index, options)?;
    let frame = fit_frame(resolved.frame, target_ratio)?;
    resolved.frame = frame;
    let scale = ((target_width - 0.1) / frame.size.x).clamp(0.1, 8.0);
    image_dimensions(frame, scale, total_pixels)?;
    let render_options = ExportOptions {
        dark: options.dark,
        scale,
        transparent: false,
        ..options.clone()
    };
    let svg = render::svg_resolved(sheet, &render_options, &resolved)?;
    render::raster_svg(&svg, scale, ImageFormat::Png)
}

/// Extends a slide viewport to the selected deck ratio without stretching or cropping.
pub fn fit_presentation_frame(
    frame: Rect,
    ratio: PresentationAspectRatio,
) -> Result<Rect, IoError> {
    fit_frame(frame, ratio.dimensions().0)
}

fn fit_frame(frame: Rect, aspect_ratio: f32) -> Result<Rect, IoError> {
    if !aspect_ratio.is_finite()
        || aspect_ratio <= 0.0
        || !frame.size.x.is_finite()
        || !frame.size.y.is_finite()
        || frame.size.x <= 0.0
        || frame.size.y <= 0.0
    {
        return Err(IoError::Invalid(
            "presentation slide frame has invalid dimensions".into(),
        ));
    }
    let mut size = frame.size;
    if size.x / size.y > aspect_ratio {
        size.y = size.x / aspect_ratio;
    } else {
        size.x = size.y * aspect_ratio;
    }
    let fitted = Rect::from_center_size(frame.center(), size);
    if !fitted.min.x.is_finite()
        || !fitted.min.y.is_finite()
        || !fitted.size.x.is_finite()
        || !fitted.size.y.is_finite()
    {
        return Err(IoError::PresentationLimit("slide frame dimensions"));
    }
    Ok(fitted)
}

pub(crate) fn xml_escape(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            matches!(*character, '\t' | '\n' | '\r')
                || ('\u{20}'..='\u{D7FF}').contains(character)
                || ('\u{E000}'..='\u{FFFD}').contains(character)
                || ('\u{10000}'..='\u{10FFFF}').contains(character)
        })
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn content_types(slide_count: usize) -> String {
    let mut xml = String::from(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#,
    );
    xml.push_str(r#"<Default Extension="jpg" ContentType="image/jpeg"/>"#);
    for index in 1..=slide_count {
        xml.push_str(&format!(
            r#"<Override PartName="/ppt/slides/slide{index}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#
        ));
    }
    xml.push_str("</Types>");
    xml
}

fn root_relationships() -> &'static str {
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#
}

fn presentation_xml(
    slide_count: usize,
    width_emu: u64,
    height_emu: u64,
    size_type: &str,
) -> String {
    let mut slides = String::new();
    for index in 0..slide_count {
        slides.push_str(&format!(
            r#"<p:sldId id="{}" r:id="rId{}"/>"#,
            256 + index,
            index + 2
        ));
    }
    format!(
        r#"<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{slides}</p:sldIdLst><p:sldSz cx="{width_emu}" cy="{height_emu}" type="{size_type}"/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle/></p:presentation>"#
    )
}

fn presentation_relationships(slide_count: usize) -> String {
    let mut xml = String::from(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>"#,
    );
    for index in 0..slide_count {
        xml.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{}.xml"/>"#,
            index + 2,
            index + 1
        ));
    }
    xml.push_str("</Relationships>");
    xml
}

fn slide_master_xml() -> &'static str {
    r#"<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMap accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" bg1="lt1" bg2="lt2" folHlink="folHlink" hlink="hlink" tx1="dk1" tx2="dk2"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#
}

fn slide_master_relationships() -> &'static str {
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#
}

fn slide_layout_xml() -> &'static str {
    r#"<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank" preserve="1"><p:cSld name="Blank"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#
}

fn slide_layout_relationships() -> &'static str {
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#
}

fn slide_xml(title: &str, shapes: &str, background: &str) -> String {
    let title = xml_escape(title);
    format!(
        r#"<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld name="{title}"><p:bg><p:bgPr>{background}<a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#
    )
}

fn theme_xml() -> &'static str {
    r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Thinoda"><a:themeElements><a:clrScheme name="Thinoda"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="Thinoda"><a:majorFont><a:latin typeface="Aptos Display"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Aptos"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Thinoda"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#
}

/// Exports resolved slides, in presentation order, as a multi-page PDF.
pub fn export_presentation_pdf(sheet: &Sheet, options: &ExportOptions) -> Result<Vec<u8>, IoError> {
    sheet.validate().map_err(IoError::Invalid)?;
    validate_slides(sheet)?;
    validate_scale(options.scale)?;

    let mut total_pixels = 0;
    let mut pages = Vec::with_capacity(sheet.presentation.slides.len());
    let mut source_bytes = 0usize;
    for index in 0..sheet.presentation.slides.len() {
        let resolved = resolve_slide(sheet, index, options)?;
        image_dimensions(resolved.frame, options.scale, &mut total_pixels)?;
        let page_width = resolved.frame.size.x * 0.75;
        let page_height = resolved.frame.size.y * 0.75;
        if page_width > MAX_PRESENTATION_PAGE_POINTS || page_height > MAX_PRESENTATION_PAGE_POINTS {
            return Err(IoError::PresentationLimit("PDF page dimensions"));
        }
        let mut opaque_options = options.clone();
        opaque_options.transparent = false;
        let svg = render::svg_resolved(sheet, &opaque_options, &resolved)?;
        source_bytes = source_bytes.saturating_add(svg.len());
        if source_bytes > MAX_PRESENTATION_ARCHIVE_BYTES {
            return Err(IoError::PresentationLimit("PDF source size"));
        }
        pages.push((svg, page_width, page_height));
    }
    render::pdf_svg_pages(&pages, MAX_PRESENTATION_ARCHIVE_BYTES)
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
        // This fixture represents a pre-redesign saved deck.
        for slide in &mut sheet.presentation.slides {
            slide.scope = PresentationScope::Context;
            slide.auto_frame = false;
        }
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
        assert_eq!(pdf.matches("/Type /Page\n").count(), 2);
        assert!(pdf.contains("/MediaBox [0 0 135 105]"));
        assert!(pdf.contains("/MediaBox [0 0 135 120]"));
        assert!(!pdf.contains("/Subtype /Image"));
        assert!(!pdf.contains("/DCTDecode"));
        assert_eq!(sheet, before);
    }

    #[test]
    fn pptx_contains_well_formed_parts_embedded_slides_and_selected_ratio() {
        let pptx_format =
            crate::Format::from_path(std::path::Path::new("presentation.PPTX")).unwrap();
        assert_eq!(pptx_format, crate::Format::PowerPoint);
        assert!(pptx_format.can_export());
        assert!(!pptx_format.can_import());

        let mut sheet = sample_sheet();
        sheet.presentation.slides[0].title = r#"Roadmap & <"delivery">"#.into();
        let before = sheet.clone();

        for (ratio, size_type, width, height) in [
            (
                PresentationAspectRatio::Widescreen16x9,
                "screen16x9",
                "12192000",
                "6858000",
            ),
            (
                PresentationAspectRatio::Standard4x3,
                "screen4x3",
                "9144000",
                "6858000",
            ),
        ] {
            let options = ExportOptions {
                presentation_aspect_ratio: ratio,
                ..ExportOptions::default()
            };
            let bytes = export_presentation_pptx(&sheet, &options).unwrap();
            let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
            let names: std::collections::HashSet<_> = (0..archive.len())
                .map(|index| archive.by_index(index).unwrap().name().unwrap().to_string())
                .collect();
            for name in [
                "[Content_Types].xml",
                "_rels/.rels",
                "ppt/presentation.xml",
                "ppt/_rels/presentation.xml.rels",
                "ppt/slideMasters/slideMaster1.xml",
                "ppt/slideMasters/_rels/slideMaster1.xml.rels",
                "ppt/slideLayouts/slideLayout1.xml",
                "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
                "ppt/theme/theme1.xml",
                "ppt/slides/slide1.xml",
                "ppt/slides/slide2.xml",
                "ppt/slides/_rels/slide1.xml.rels",
                "ppt/slides/_rels/slide2.xml.rels",
            ] {
                assert!(names.contains(name), "missing PPTX part {name}");
            }
            assert!(!names.iter().any(|name| name.starts_with("ppt/media/")));

            for index in 0..archive.len() {
                let name = archive.by_index(index).unwrap().name().unwrap().to_string();
                if name.ends_with(".xml") || name.ends_with(".rels") {
                    let mut xml = Vec::new();
                    std::io::Read::read_to_end(&mut archive.by_name(&name).unwrap(), &mut xml)
                        .unwrap();
                    let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
                    loop {
                        match reader.read_event() {
                            Ok(quick_xml::events::Event::Eof) => break,
                            Ok(_) => {}
                            Err(error) => panic!("malformed XML in {name}: {error}"),
                        }
                    }
                }
            }

            let mut presentation = String::new();
            std::io::Read::read_to_string(
                &mut archive.by_name("ppt/presentation.xml").unwrap(),
                &mut presentation,
            )
            .unwrap();
            assert!(presentation.contains(&format!(
                r#"<p:sldSz cx="{width}" cy="{height}" type="{size_type}"/>"#
            )));
            assert_eq!(presentation.matches("<p:sldId ").count(), 2);

            let mut master = String::new();
            std::io::Read::read_to_string(
                &mut archive
                    .by_name("ppt/slideMasters/slideMaster1.xml")
                    .unwrap(),
                &mut master,
            )
            .unwrap();
            let mut reader = quick_xml::Reader::from_reader(master.as_bytes());
            let mut layouts = 0;
            loop {
                match reader.read_event().unwrap() {
                    quick_xml::events::Event::Empty(element)
                        if element.name().as_ref() == "p:sldLayoutId" =>
                    {
                        layouts += 1;
                        let attributes: std::collections::BTreeMap<_, _> = element
                            .attributes()
                            .map(|attribute| {
                                let attribute = attribute.unwrap();
                                (
                                    attribute.key.as_ref().to_owned(),
                                    attribute
                                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                        .unwrap()
                                        .into_owned(),
                                )
                            })
                            .collect();
                        // ST_SlideLayoutId is not a shape ID or relationship index.
                        assert!(attributes["id"].parse::<u32>().unwrap() >= 2_147_483_648);
                        assert_ne!(
                            attributes["id"], "2147483648",
                            "layout must not reuse master ID"
                        );
                        assert_eq!(attributes["r:id"], "rId1");
                    }
                    quick_xml::events::Event::Eof => break,
                    _ => {}
                }
            }
            assert_eq!(layouts, 1);

            let mut first_slide = String::new();
            std::io::Read::read_to_string(
                &mut archive.by_name("ppt/slides/slide1.xml").unwrap(),
                &mut first_slide,
            )
            .unwrap();
            assert!(first_slide.contains("Roadmap &amp; &lt;&quot;delivery&quot;&gt;"));
            assert!(first_slide.contains("<a:t>Branch</a:t>"));
            assert!(first_slide.contains("<a:custGeom>"));
            assert!(!first_slide.contains("<p:cxnSp>"));
            assert!(first_slide.contains("<a:cubicBezTo>"));
            assert!(!first_slide.contains("<p:pic>"));
            let preview = render_presentation_pptx_slide_png(&sheet, 0, &options).unwrap();
            let preview_dimensions = image::load_from_memory(&preview).unwrap().dimensions();
            assert_eq!(preview_dimensions.0, 640);
            assert_eq!(
                preview_dimensions.0 as f32 / preview_dimensions.1 as f32,
                ratio.dimensions().0
            );
            assert_eq!(sheet, before);
        }
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
            scope: PresentationScope::Context,
            auto_frame: false,
            hidden_topics: Vec::new(),
            max_depth: None,
        });
        assert!(export_presentation_pdf(&sheet, &ExportOptions::default()).is_err());
        assert!(export_presentation_png_zip(&sheet, &ExportOptions::default()).is_err());
    }
}
