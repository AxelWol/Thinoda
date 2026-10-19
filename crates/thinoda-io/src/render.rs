use crate::{ExportOptions, IoError};
use std::io::Cursor;
use thinoda_core::{
    Workbook,
    geometry::{Color, Vec2},
    layout::{ApproxMeasure, ConnectorKind, layout_sheet},
    model::TopicShape,
    theme::Theme,
};

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
    let s = sheet(w, o);
    let layout = layout_sheet(s, &ApproxMeasure, o.dark);
    let pad = o.padding.max(0.0);
    let b = layout.bounds.expand(pad);
    let max = b.max();
    let width = max.x - b.min.x;
    let height = max.y - b.min.y;
    let bg = Theme::for_settings(&s.theme).background(&s.theme, o.dark);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{width:.2}\" height=\"{height:.2}\" viewBox=\"{:.2} {:.2} {width:.2} {height:.2}\">",
        b.min.x, b.min.y
    );
    out.push_str("<defs><marker id=\"arrow\" markerWidth=\"8\" markerHeight=\"8\" refX=\"6\" refY=\"3\" orient=\"auto\"><path d=\"M0,0 L0,6 L7,3 z\" fill=\"#777777\"/></marker></defs>");
    if !o.transparent {
        out.push_str(&format!("<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{width:.2}\" height=\"{height:.2}\" fill=\"{}\"/>",b.min.x,b.min.y,col(bg)));
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
        let title = &s.topics[id].title;
        let lines = ApproxMeasure::wrap(
            title,
            style.font_size,
            style.bold,
            tl.title_rect.size.x.max(style.font_size),
        );
        let line_h = style.font_size * ApproxMeasure::LINE_HEIGHT;
        let mut y = tl.title_rect.center().y - (lines.len() as f32 - 1.0) * line_h / 2.0;
        let weight = if style.bold { "bold" } else { "normal" };
        let font = style
            .font_family
            .as_deref()
            .unwrap_or("Segoe UI, Helvetica, Arial, sans-serif");
        for (line, _) in lines {
            out.push_str(&format!("<text x=\"{:.2}\" y=\"{y:.2}\" text-anchor=\"middle\" dominant-baseline=\"middle\" font-family=\"{}\" font-size=\"{:.2}\" font-weight=\"{weight}\" fill=\"{}\">{}</text>",tl.title_rect.center().x,esc(font),style.font_size,col(style.text_color),esc(&line)));
            y += line_h;
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
    let opts = resvg::usvg::Options {
        fontdb: crate::fonts::database(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(&source, &opts)
        .map_err(|e| IoError::Invalid(format!("SVG parse: {e}")))?;
    let scale = if o.scale.is_finite() {
        o.scale.clamp(0.1, 8.0)
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
        if !o.transparent { // preserve canvas fill from SVG; renderer already paints it
        }
        image::DynamicImage::ImageRgba8(std::mem::take(&mut img))
            .write_to(&mut out, fmt)
            .map_err(|e| IoError::Invalid(e.to_string()))?;
    }
    Ok(out.into_inner())
}
pub(crate) fn pdf(w: &Workbook, o: &ExportOptions) -> Result<Vec<u8>, IoError> {
    let mut opts = o.clone();
    opts.transparent = false;
    opts.scale = 1.0;
    let jpg = raster(w, &opts, image::ImageFormat::Jpeg)?;
    let decoded = image::load_from_memory(&jpg)
        .map_err(|e| IoError::Invalid(e.to_string()))?
        .to_rgb8();
    let (width, height) = decoded.dimensions();
    Ok(image_pdf(&jpg, width, height, width as f32, height as f32))
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
    let mut objects: Vec<Vec<u8>> = Vec::new();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_width:.3} {page_height:.3}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>").into_bytes());
    let mut image_obj=format!("<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",jpg.len()).into_bytes();
    image_obj.extend_from_slice(jpg);
    image_obj.extend_from_slice(b"\nendstream");
    objects.push(image_obj);
    let stream = format!("q {page_width:.3} 0 0 {page_height:.3} 0 0 cm /Im0 Do Q");
    objects.push(
        format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        )
        .into_bytes(),
    );
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
    #[test]
    fn svg_escapes_topic_text() {
        let w = Workbook::new(thinoda_core::Sheet::new("S", "<root>"));
        let xml = svg(&w, &ExportOptions::default()).unwrap();
        assert!(xml.contains("&lt;root&gt;"));
    }
}
