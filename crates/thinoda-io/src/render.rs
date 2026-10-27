use crate::{ExportOptions, IoError};
use std::io::Cursor;
use thinoda_core::{
    Rect, Sheet, TopicId, Workbook,
    geometry::{Color, Vec2},
    layout::{ConnectorKind, layout_sheet},
    model::TopicShape,
    theme::Theme,
};

mod text;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn col(c: Color) -> String {
    c.to_hex()
}
fn xy(p: Vec2) -> String {
    format!("{:.2},{:.2}", p.x, p.y)
}
fn sheet<'a>(w: &'a Workbook, o: &ExportOptions) -> &'a thinoda_core::Sheet {
    &w.sheets[o.sheet.unwrap_or(w.active_sheet).min(w.sheets.len() - 1)]
}
pub(crate) fn svg(w: &Workbook, o: &ExportOptions) -> Result<String, IoError> {
    svg_with_fonts(w, o, crate::fonts::database())
}

fn svg_with_fonts(
    w: &Workbook,
    o: &ExportOptions,
    fontdb: std::sync::Arc<resvg::usvg::fontdb::Database>,
) -> Result<String, IoError> {
    let s = sheet(w, o);
    svg_sheet_with_fonts(s, o, fontdb, None)
}

pub(crate) fn svg_with_frame(
    s: &thinoda_core::Sheet,
    o: &ExportOptions,
    frame: thinoda_core::Rect,
) -> Result<String, IoError> {
    svg_sheet_with_fonts(s, o, crate::fonts::database(), Some(frame))
}

pub(crate) fn branch_frames(
    sheet: &Sheet,
    roots: &[TopicId],
    dark: bool,
) -> Result<Vec<Rect>, IoError> {
    for root in roots {
        if sheet.main_branch_of(*root).is_none() {
            return Err(IoError::Invalid(format!(
                "topic {root} is not in the central topic tree"
            )));
        }
    }
    let text = text::ExportText::new(sheet, dark, crate::fonts::database())?;
    let layout = layout_sheet(sheet, &text, dark);
    roots
        .iter()
        .map(|root| {
            let owned: std::collections::HashSet<_> =
                sheet.owned_subtree_ids(*root).into_iter().collect();
            let mut bounds = owned
                .iter()
                .filter_map(|id| layout.topics.get(id).map(|topic| topic.rect))
                .reduce(|bounds, rect| bounds.union(&rect))
                .ok_or_else(|| IoError::Invalid(format!("branch {root} has no visible topics")))?;
            for summary in &layout.summaries {
                if owned.contains(&summary.topic)
                    || sheet
                        .summaries
                        .iter()
                        .any(|source| source.id == summary.id && owned.contains(&source.parent))
                {
                    bounds = bounds.union(&summary.range_rect);
                }
            }
            for boundary in &layout.boundaries {
                if sheet
                    .boundaries
                    .iter()
                    .any(|source| source.id == boundary.id && owned.contains(&source.parent))
                {
                    bounds = bounds.union(&boundary.rect);
                }
            }
            Ok(bounds.expand(80.0))
        })
        .collect()
}

fn svg_sheet_with_fonts(
    s: &thinoda_core::Sheet,
    o: &ExportOptions,
    fontdb: std::sync::Arc<resvg::usvg::fontdb::Database>,
    frame: Option<thinoda_core::Rect>,
) -> Result<String, IoError> {
    let image_dimensions = thinoda_core::images::validate_visual(s).map_err(|error| {
        IoError::Invalid(format!("embedded resources cannot be rendered: {error}"))
    })?;
    let text = text::ExportText::new(s, o.dark, fontdb)?;
    let layout = layout_sheet(s, &text, o.dark);
    let pad = o.padding.max(0.0);
    let b = frame.unwrap_or_else(|| layout.bounds.expand(pad));
    let max = b.max();
    let width = max.x - b.min.x;
    let height = max.y - b.min.y;
    let bg = Theme::for_settings(&s.theme).background(&s.theme, o.dark);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{width:.2}\" height=\"{height:.2}\" viewBox=\"{:.2} {:.2} {width:.2} {height:.2}\">",
        b.min.x, b.min.y
    );
    out.push_str("<defs><marker id=\"arrow\" markerWidth=\"8\" markerHeight=\"8\" refX=\"6\" refY=\"3\" orient=\"auto\"><path d=\"M0,0 L0,6 L7,3 z\" fill=\"#777777\"/></marker></defs>");
    // Embed each referenced blob once, even when thousands of topics share it.
    out.push_str("<defs>");
    for (reference, size) in &image_dimensions {
        use base64::Engine;
        let resource = s.resources.get(reference).map_err(IoError::Invalid)?;
        let id = reference
            .strip_prefix(thinoda_core::resources::RESOURCE_PREFIX)
            .ok_or_else(|| IoError::Invalid("not an embedded image".into()))?;
        let data = base64::engine::general_purpose::STANDARD.encode(resource.bytes());
        out.push_str(&format!(
            "<image id=\"image-{}\" width=\"{}\" height=\"{}\" xlink:href=\"data:{};base64,{}\"/>",
            esc(id),
            size.x,
            size.y,
            resource.media_type,
            data
        ));
    }
    out.push_str("</defs>");
    if !o.transparent {
        out.push_str(&format!("<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{width:.2}\" height=\"{height:.2}\" fill=\"{}\"/>",b.min.x,b.min.y,col(bg)));
    }
    for zone in &s.zones {
        let r = zone.frame;
        let color = Color::rgb(0x5D, 0x7A, 0x91);
        let fill = Color::rgba(color.r, color.g, color.b, 20);
        out.push_str(&format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"4\" fill=\"{}\" stroke=\"{}\"/>",
            r.min.x,
            r.min.y,
            r.size.x,
            r.size.y,
            col(fill),
            col(color)
        ));
        out.push_str(&format!(
            "<text x=\"{:.2}\" y=\"{:.2}\" fill=\"{}\" font-size=\"12\">{}</text>",
            r.min.x + 8.0,
            r.min.y + 17.0,
            col(color),
            esc(&zone.name)
        ));
    }
    for boundary in &layout.boundaries {
        let r = boundary.rect;
        let title = s
            .boundaries
            .iter()
            .find(|x| x.id == boundary.id)
            .and_then(|x| x.title.as_deref())
            .unwrap_or("");
        let color = s
            .boundaries
            .iter()
            .find(|x| x.id == boundary.id)
            .and_then(|b| b.color)
            .unwrap_or(Color::rgb(0x8A, 0x9B, 0xB0));
        let fill = Color::rgba(
            color.r,
            color.g,
            color.b,
            (color.a as f32 * 0.12).round() as u8,
        );
        out.push_str(&format!("<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"10\" fill=\"{}\" stroke=\"{}\" stroke-dasharray=\"5 4\"/>",r.min.x,r.min.y,r.size.x,r.size.y,col(fill),col(color)));
        if !title.is_empty() {
            out.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" fill=\"#777777\" font-size=\"12\">{}</text>",
                r.min.x + 6.0,
                r.min.y + 14.0,
                esc(title)
            ));
        }
    }
    for c in &layout.connectors {
        let d = match c.kind {
            ConnectorKind::Straight => format!("M {} L {}", xy(c.start), xy(c.end)),
            ConnectorKind::Elbow
            | ConnectorKind::RoundedElbow
            | ConnectorKind::Tree
            | ConnectorKind::Brace => {
                if c.vertical {
                    let mid = (c.start.y + c.end.y) / 2.0;
                    format!(
                        "M {} L {:.2},{mid:.2} L {:.2},{mid:.2} L {}",
                        xy(c.start),
                        c.start.x,
                        c.end.x,
                        xy(c.end)
                    )
                } else {
                    let mid = (c.start.x + c.end.x) / 2.0;
                    format!(
                        "M {} L {mid:.2},{:.2} L {mid:.2},{:.2} L {}",
                        xy(c.start),
                        c.start.y,
                        c.end.y,
                        xy(c.end)
                    )
                }
            }
            ConnectorKind::Curve => {
                let dx = c.end.x - c.start.x;
                let dy = c.end.y - c.start.y;
                let k = if c.vertical {
                    dy.abs() * 0.5
                } else {
                    dx.abs() * 0.5
                };
                let p1 = if c.vertical {
                    Vec2::new(c.start.x, c.start.y + dy.signum() * k)
                } else {
                    Vec2::new(c.start.x + dx.signum() * k, c.start.y)
                };
                let p2 = if c.vertical {
                    Vec2::new(c.end.x, c.end.y - dy.signum() * k)
                } else {
                    Vec2::new(c.end.x - dx.signum() * k, c.end.y)
                };
                format!("M {} C {} {} {}", xy(c.start), xy(p1), xy(p2), xy(c.end))
            }
        };
        out.push_str(&format!("<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\"/>",col(c.color),c.width));
    }
    for r in &layout.relationships {
        let title = s
            .relationships
            .iter()
            .find(|x| x.id == r.id)
            .map(|x| x.title.as_str())
            .unwrap_or("");
        out.push_str(&format!("<path d=\"M {} C {} {} {}\" fill=\"none\" stroke=\"#777777\" stroke-dasharray=\"5 4\" marker-end=\"url(#arrow)\"/>",xy(r.start),xy(r.c1),xy(r.c2),xy(r.end)));
        if !title.is_empty() {
            out.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" fill=\"#666666\" font-size=\"12\">{}</text>",
                r.mid.x,
                r.mid.y,
                esc(title)
            ));
        }
    }
    for summary in &layout.summaries {
        out.push_str(&format!(
            "<path d=\"M {} L {} L {} L {}\" fill=\"none\" stroke=\"#777777\" stroke-width=\"2\"/>",
            xy(summary.start),
            xy(summary.tip),
            xy(summary.end),
            xy(summary.tip)
        ));
    }
    for id in &layout.order {
        let Some(tl) = layout.topics.get(id) else {
            continue;
        };
        let r = tl.rect;
        let style = &tl.style;
        let fill = style.fill.map(col).unwrap_or_else(|| "none".into());
        let stroke = style
            .border_color
            .map(col)
            .unwrap_or_else(|| col(style.line_color));
        let sw = if style.border_width > 0.0 {
            style.border_width
        } else {
            style.line_width
        };
        let shape = match style.shape {
            TopicShape::Ellipse => format!(
                "<ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\"",
                r.center().x,
                r.center().y,
                r.size.x / 2.0,
                r.size.y / 2.0
            ),
            TopicShape::Diamond => format!(
                "<polygon points=\"{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}\"",
                r.center().x,
                r.min.y,
                r.max().x,
                r.center().y,
                r.center().x,
                r.max().y,
                r.min.x,
                r.center().y
            ),
            TopicShape::Underline => format!(
                "<path d=\"M {:.2},{:.2} L {:.2},{:.2}\"",
                r.min.x,
                r.max().y,
                r.max().x,
                r.max().y
            ),
            TopicShape::None => format!(
                "<path d=\"M {:.2},{:.2} L {:.2},{:.2}\"",
                r.min.x,
                r.max().y,
                r.max().x,
                r.max().y
            ),
            TopicShape::Rect => format!(
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\"",
                r.min.x, r.min.y, r.size.x, r.size.y
            ),
            _ => format!(
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{}\"",
                r.min.x,
                r.min.y,
                r.size.x,
                r.size.y,
                if style.shape == TopicShape::Capsule {
                    r.size.y / 2.0
                } else {
                    6.0
                }
            ),
        };
        if style.shape == TopicShape::None || style.shape == TopicShape::Underline {
            if style.fill.is_some() {
                out.push_str(&format!("<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"4\" fill=\"{fill}\"/>",
                    r.min.x, r.min.y, r.size.x, r.size.y));
            }
            out.push_str(&format!(
                "{shape} fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{sw:.2}\"/>",
            ));
        } else {
            out.push_str(&format!(
                "{shape} fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{sw:.2}\"/>"
            ));
        }
        if let Some(rect) = tl.image_rect
            && let Some(reference) = s.topic(*id).and_then(|topic| topic.image.as_deref())
        {
            let size = image_dimensions
                .get(reference)
                .ok_or_else(|| IoError::Invalid("missing image dimensions".into()))?;
            let scale = (rect.size.x / size.x).min(rect.size.y / size.y);
            let min = rect.center() - *size * (scale / 2.0);
            let id = reference
                .strip_prefix(thinoda_core::resources::RESOURCE_PREFIX)
                .ok_or_else(|| IoError::Invalid("not an embedded image".into()))?;
            out.push_str(&format!(
                "<use xlink:href=\"#image-{}\" transform=\"translate({:.4} {:.4}) scale({scale:.8})\"/>",
                esc(id), min.x, min.y
            ));
        }
        let block = text.topic(*id);
        let mut y = tl.title_rect.min.y;
        let weight = if style.bold { "bold" } else { "normal" };
        let font = style
            .font_family
            .as_deref()
            .unwrap_or("Segoe UI, Helvetica, Arial, sans-serif");
        for (index, line) in block.lines.iter().enumerate() {
            let x = tl.title_rect.center().x - line.bounds.center().x;
            let baseline = y + (block.line_height - line.bounds.size.y) / 2.0 - line.bounds.min.y;
            // Standalone SVG viewers may substitute fonts; retain the measured width.
            let length = if line.text.is_empty() {
                String::new()
            } else {
                format!(
                    " textLength=\"{:.2}\" lengthAdjust=\"spacingAndGlyphs\"",
                    line.bounds.size.x
                )
            };
            out.push_str(&format!("<text id=\"topic-{id}-line-{index}\" x=\"{x:.2}\" y=\"{baseline:.2}\" font-family=\"{}\" font-size=\"{:.2}\" font-weight=\"{weight}\" fill=\"{}\"{length}>{}</text>",esc(font),style.font_size,col(style.text_color),esc(&line.text)));
            y += block.line_height;
        }
        if let Some((label, nr)) = &tl.number {
            out.push_str(&format!("<text class=\"topic-number\" x=\"{:.2}\" y=\"{:.2}\" dominant-baseline=\"central\" font-family=\"{}\" font-size=\"{:.2}\" font-weight=\"{weight}\" fill=\"{}\">{}</text>",nr.min.x,nr.center().y,esc(font),style.font_size,col(style.text_color),esc(label)));
        }
        for (index, (mark, mr)) in tl.markers.iter().enumerate() {
            if let Some(color) = s.topics[id].marker_colors.get(&index) {
                out.push_str(&colored_marker_svg(mark, *mr, *color));
                continue;
            }
            let g = thinoda_core::markers::glyph(mark);
            out.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"14\">{}</text>",
                mr.center().x,
                mr.center().y,
                esc(g)
            ));
        }
        for (label, lr) in &tl.labels {
            out.push_str(&format!("<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"5\" fill=\"#EEEEEE\"/><text x=\"{:.2}\" y=\"{:.2}\" font-size=\"10\" text-anchor=\"middle\" fill=\"#333333\">{}</text>",lr.min.x,lr.min.y,lr.size.x,lr.size.y,lr.center().x,lr.center().y,esc(label)));
        }
    }
    out.push_str("</svg>");
    Ok(out)
}

fn colored_marker_svg(id: &str, r: thinoda_core::Rect, color: Color) -> String {
    use thinoda_core::markers;
    let c = r.center();
    let rad = r.size.x.min(r.size.y) / 2.0;
    let ink = col(color);
    let detail = col(Color::highest_contrast_on(color.over(Color::WHITE)));
    let text = |value: &str, size: f32, fill: &str| {
        format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" dominant-baseline=\"central\" font-family=\"Segoe UI, sans-serif\" font-size=\"{}\" fill=\"{}\">{}</text>",
            c.x,
            c.y,
            size,
            fill,
            esc(value)
        )
    };
    match markers::group_of(id) {
        "priority" => format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{rad}\" fill=\"{ink}\"/>{}",
            c.x,
            c.y,
            text(
                id.strip_prefix("priority-").unwrap_or("1"),
                rad * 1.3,
                &detail
            )
        ),
        "task" => {
            let fraction = markers::task_fraction(id);
            let mut svg = format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"{ink}\" stroke-width=\"1.2\"/>",
                c.x,
                c.y,
                rad - 0.5,
                if fraction >= 1.0 { &ink } else { &detail }
            );
            if fraction >= 1.0 {
                svg.push_str(&format!("<polyline points=\"{} {} {}\" fill=\"none\" stroke=\"{detail}\" stroke-width=\"1.5\"/>",
                    xy(c + Vec2::new(-rad * 0.45, 0.0)), xy(c + Vec2::new(-rad * 0.1, rad * 0.35)),
                    xy(c + Vec2::new(rad * 0.5, -rad * 0.35))));
            } else if fraction > 0.0 {
                let steps = (24.0 * fraction).ceil() as usize + 1;
                let mut points = vec![xy(c)];
                for i in 0..=steps {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * fraction * i as f32 / steps as f32;
                    points.push(xy(c + Vec2::new(angle.cos(), angle.sin()) * (rad - 1.5)));
                }
                svg.push_str(&format!(
                    "<polygon points=\"{}\" fill=\"{ink}\"/>",
                    points.join(" ")
                ));
            }
            svg
        }
        "flag" => {
            let x = r.min.x + r.size.x * 0.25;
            format!(
                "<path d=\"M {x},{} L {x},{}\" stroke=\"{ink}\" stroke-width=\"1.4\"/><polygon points=\"{x},{} {},{} {x},{}\" fill=\"{ink}\"/>",
                r.min.y + 1.0,
                r.max().y - 1.0,
                r.min.y + 1.5,
                r.max().x - 1.0,
                r.min.y + r.size.y * 0.3,
                r.min.y + r.size.y * 0.6
            )
        }
        "star" => {
            let points = (0..10)
                .map(|i| {
                    let angle =
                        -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                    let radius = if i % 2 == 0 { rad } else { rad * 0.45 };
                    xy(c + Vec2::new(angle.cos(), angle.sin()) * radius)
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("<polygon points=\"{points}\" fill=\"{ink}\"/>")
        }
        _ => text(markers::glyph(id), rad * 1.6, &ink),
    }
}
pub(crate) fn raster(
    w: &Workbook,
    o: &ExportOptions,
    fmt: image::ImageFormat,
) -> Result<Vec<u8>, IoError> {
    let source = svg(w, o)?;
    raster_svg(&source, o.scale, fmt)
}

pub(crate) fn raster_svg(
    source: &str,
    scale: f32,
    fmt: image::ImageFormat,
) -> Result<Vec<u8>, IoError> {
    let opts = resvg::usvg::Options {
        fontdb: crate::fonts::database(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(source, &opts)
        .map_err(|e| IoError::Invalid(format!("SVG parse: {e}")))?;
    let scale = if scale.is_finite() {
        scale.clamp(0.1, 8.0)
    } else {
        1.0
    };
    let size = tree.size();
    let width = (size.width() * scale).ceil().clamp(1.0, 16_384.0) as u32;
    let height = (size.height() * scale).ceil().clamp(1.0, 16_384.0) as u32;
    let mut pix = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| IoError::Invalid("image dimensions exceed limits".into()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pix.as_mut(),
    );
    let rgba = pix.data().to_vec();
    let mut img = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| IoError::Invalid("invalid rendered image".into()))?;
    let mut out = Cursor::new(Vec::new());
    if fmt == image::ImageFormat::Jpeg {
        let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
        image::DynamicImage::ImageRgb8(rgb)
            .write_to(&mut out, fmt)
            .map_err(|e| IoError::Invalid(e.to_string()))?;
    } else {
        image::DynamicImage::ImageRgba8(std::mem::take(&mut img))
            .write_to(&mut out, fmt)
            .map_err(|e| IoError::Invalid(e.to_string()))?;
    }
    Ok(out.into_inner())
}
pub(crate) fn pdf(w: &Workbook, o: &ExportOptions) -> Result<Vec<u8>, IoError> {
    let mut opts = o.clone();
    opts.transparent = false;
    let source = svg(w, &opts)?;
    pdf_svg(&source)
}

pub(crate) fn pdf_svg(source: &str) -> Result<Vec<u8>, IoError> {
    let tree = resvg::usvg::Tree::from_str(
        source,
        &resvg::usvg::Options {
            fontdb: crate::fonts::database(),
            ..Default::default()
        },
    )
    .map_err(|e| IoError::Invalid(format!("SVG parse: {e}")))?;
    // Outline with the same font resolver as raster export. The PDF converter's
    // usvg version differs, so pass normalized SVG rather than incompatible trees.
    let outlined = tree.to_string(&resvg::usvg::WriteOptions::default());
    let tree = svg2pdf::usvg::Tree::from_str(&outlined, &Default::default())
        .map_err(|e| IoError::Invalid(format!("PDF SVG parse: {e}")))?;
    svg2pdf::to_pdf(&tree, Default::default(), Default::default())
        .map_err(|e| IoError::Invalid(format!("PDF conversion: {e}")))
}

/// Wraps an opaque JPEG in a single PDF page. Page dimensions are PDF points,
/// independent of raster resolution (printing uses physical paper dimensions).
pub(crate) fn image_pdf(
    jpg: &[u8],
    width: u32,
    height: u32,
    page_width: f32,
    page_height: f32,
) -> Vec<u8> {
    image_pdf_pages(&[PdfImagePage {
        jpg,
        width,
        height,
        page_width,
        page_height,
    }])
}

pub(crate) struct PdfImagePage<'a> {
    pub jpg: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub page_width: f32,
    pub page_height: f32,
}

pub(crate) fn image_pdf_pages(pages: &[PdfImagePage<'_>]) -> Vec<u8> {
    let mut objects = Vec::with_capacity(2 + pages.len() * 3);
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let page_refs = (0..pages.len())
        .map(|index| format!("{} 0 R", 3 + index * 3))
        .collect::<Vec<_>>()
        .join(" ");
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{page_refs}] /Count {} >>",
            pages.len()
        )
        .into_bytes(),
    );
    for (index, page) in pages.iter().enumerate() {
        let page_id = 3 + index * 3;
        let image_id = page_id + 1;
        let content_id = page_id + 2;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.3} {:.3}] /Resources << /XObject << /Im0 {image_id} 0 R >> >> /Contents {content_id} 0 R >>",
            page.page_width, page.page_height
        ).into_bytes());
        let mut image_obj = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            page.width,
            page.height,
            page.jpg.len()
        )
        .into_bytes();
        image_obj.extend_from_slice(page.jpg);
        image_obj.extend_from_slice(b"\nendstream");
        objects.push(image_obj);
        let stream = format!(
            "q {:.3} 0 0 {:.3} 0 0 cm /Im0 Do Q",
            page.page_width, page.page_height
        );
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                stream.len(),
                stream
            )
            .into_bytes(),
        );
    }
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend_from_slice(obj);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for off in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use thinoda_core::{Rect, Sheet};

    #[test]
    fn svg_escapes_topic_text() {
        let w = Workbook::new(thinoda_core::Sheet::new("S", "<root>"));
        let xml = svg(&w, &ExportOptions::default()).unwrap();
        assert!(xml.contains("&lt;root&gt;"));
    }

    fn assert_title_fits(
        source: &str,
        fontdb: Arc<resvg::usvg::fontdb::Database>,
        layout: &thinoda_core::layout::Layout,
        text: &text::ExportText,
    ) {
        let tree = resvg::usvg::Tree::from_str(
            source,
            &resvg::usvg::Options {
                fontdb,
                ..Default::default()
            },
        )
        .unwrap();
        for (&id, topic) in &layout.topics {
            let inset = topic.style.border_width.max(topic.style.line_width) / 2.0 + 2.0;
            let inner = topic.rect.expand(-inset);
            for (index, line) in text.topic(id).lines.iter().enumerate() {
                if line.text.is_empty() {
                    continue;
                }
                let node = tree
                    .node_by_id(&format!("topic-{id}-line-{index}"))
                    .unwrap();
                let bbox = node.bounding_box();
                let bounds = Rect::from_min_size(
                    Vec2::new(bbox.x(), bbox.y()),
                    Vec2::new(bbox.width(), bbox.height()),
                );
                for corner in [
                    bounds.min,
                    bounds.max(),
                    Vec2::new(bounds.min.x, bounds.max().y),
                    Vec2::new(bounds.max().x, bounds.min.y),
                ] {
                    assert!(
                        inner.contains(corner),
                        "{id} {:?}: {bounds:?} outside {inner:?}",
                        line.text
                    );
                    let delta = corner - inner.center();
                    let x = delta.x / (inner.size.x / 2.0);
                    let y = delta.y / (inner.size.y / 2.0);
                    match topic.style.shape {
                        TopicShape::Ellipse => assert!(x * x + y * y <= 1.0, "ellipse {id}"),
                        TopicShape::Diamond => assert!(x.abs() + y.abs() <= 1.0, "diamond {id}"),
                        TopicShape::Capsule => {
                            let radius = inner.size.y / 2.0;
                            let dx = (delta.x.abs() - (inner.size.x / 2.0 - radius)).max(0.0);
                            assert!(
                                dx * dx + delta.y * delta.y <= radius * radius,
                                "capsule {id}"
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn regression_map() -> Workbook {
        let mut s = Sheet::new("Export regression", "KI Coden");
        let branch = s.add_child(s.root, None, "6. Von der Idee zum Code");
        for title in [
            "Formen von KI-Coding",
            "Chat in VsCode",
            "GitHub Copilot CLI",
            "KERN Katalog",
            "R\u{fc}ckblick / Entwicklung von LLMs",
            "Refactoring und Produkt ab 9/25 (V1.x)",
            "Integration in die kern-ux.de Dom\u{e4}ne ab 1/26",
            "One line\n\nAn explicit third line",
            "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW",
            "",
        ] {
            s.add_child(branch, None, title);
        }
        for shape in [
            TopicShape::RoundedRect,
            TopicShape::Rect,
            TopicShape::Capsule,
            TopicShape::Ellipse,
            TopicShape::Diamond,
            TopicShape::Underline,
        ] {
            let id = s.add_child(branch, None, "Wide WWW accents \u{c4}\u{e9}gyp\nmore text");
            let style = &mut s.topics.get_mut(&id).unwrap().style;
            style.shape = Some(shape);
            style.width = Some(95.0);
            style.font_size = Some(28.0);
            style.bold = Some(true);
            style.font_family = Some("monospace".into());
            style.border_width = Some(12.0);
        }
        let narrow = s.add_child(branch, None, "W");
        s.topics.get_mut(&narrow).unwrap().style.width = Some(1.0);
        Workbook::new(s)
    }

    #[test]
    fn rendered_titles_keep_whitespace_inside_nodes_with_native_and_web_fonts() {
        let w = regression_map();
        for fontdb in [
            crate::fonts::database(),
            Arc::new(crate::fonts::build(false)),
        ] {
            for dark in [false, true] {
                let opts = ExportOptions {
                    dark,
                    ..Default::default()
                };
                let text = text::ExportText::new(w.active(), dark, fontdb.clone()).unwrap();
                let layout = layout_sheet(w.active(), &text, dark);
                let source = svg_with_fonts(&w, &opts, fontdb.clone()).unwrap();
                assert_title_fits(&source, fontdb.clone(), &layout, &text);
                let substituted = source.replace(
                    "font-family=\"Segoe UI, Helvetica, Arial, sans-serif\"",
                    "font-family=\"monospace\"",
                );
                assert_title_fits(&substituted, fontdb.clone(), &layout, &text);
                let id = w
                    .active()
                    .topics
                    .values()
                    .find(|t| t.title == "KERN Katalog")
                    .unwrap()
                    .id;
                assert_eq!(text.topic(id).lines.len(), 1);
            }
        }
    }

    #[test]
    fn drawing_reuses_measured_lines_even_when_the_last_space_was_trimmed() {
        let mut sheet = Sheet::new("S", "Root");
        let parent = sheet.add_child(sheet.root, None, "Branch");
        let id = sheet.add_child(parent, None, "Formen von KI-Coding");
        let w = Workbook::new(sheet);
        let text = text::ExportText::new(w.active(), false, crate::fonts::database()).unwrap();
        assert_eq!(text.topic(id).lines.len(), 1);
        let source = svg(&w, &Default::default()).unwrap();
        assert!(source.contains("Formen von KI-Coding</text>"));
        assert!(!source.contains(&format!("topic-{id}-line-1")));
    }

    #[test]
    fn pdf_contains_vectors_not_a_page_sized_bitmap_and_ignores_raster_scale() {
        let w = regression_map();
        let a = pdf(&w, &Default::default()).unwrap();
        let b = pdf(
            &w,
            &ExportOptions {
                scale: 8.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(a.starts_with(b"%PDF-"));
        let source = String::from_utf8_lossy(&a);
        assert!(!source.contains("/Subtype /Image"));
        assert!(!source.contains("/DCTDecode"));
        assert!(source.contains("/MediaBox"));
        assert_eq!(a, b);
    }
}
