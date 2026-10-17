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
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
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
        out.push_str(&format!("<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"10\" fill=\"none\" stroke=\"#999999\" stroke-dasharray=\"5 4\"/>",r.min.x,r.min.y,r.size.x,r.size.y));
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
        for (mark, mr) in &tl.markers {
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
pub(crate) fn raster(
    w: &Workbook,
    o: &ExportOptions,
    fmt: image::ImageFormat,
) -> Result<Vec<u8>, IoError> {
    let source = svg(w, o)?;
    let mut opts = resvg::usvg::Options::default();
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    opts.fontdb = std::sync::Arc::new(db);
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
    let mut objects: Vec<Vec<u8>> = Vec::new();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>").into_bytes());
    let mut image_obj=format!("<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",jpg.len()).into_bytes();
    image_obj.extend_from_slice(&jpg);
    image_obj.extend_from_slice(b"\nendstream");
    objects.push(image_obj);
    let stream = format!("q {width} 0 0 {height} 0 0 cm /Im0 Do Q");
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
    Ok(pdf)
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
