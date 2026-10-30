//! Native DrawingML primitives. No rasterization of slide text or geometry.

use super::text::ExportText;
use crate::presentation::xml_escape as esc;
use crate::{ExportOptions, IoError};
use std::collections::HashMap;
use thinoda_core::{
    Color, Rect, ResolvedPresentationSlide, Sheet, TopicId, TopicShape, Vec2,
    layout::{ConnectorKind, Icon},
    theme::Theme,
};

pub(crate) struct NativeSlide {
    pub shapes: String,
    /// Archive path and original PNG/JPEG bytes, only for actual topic images.
    pub media: Vec<(String, Vec<u8>)>,
    pub relationships: String,
    pub background: String,
}

struct Drawing {
    xml: String,
    next_id: u32,
    frame: Rect,
    scale: f32,
    width: u64,
    height: u64,
}

fn fill(color: Option<Color>) -> String {
    color.map_or_else(|| "<a:noFill/>".into(), |c| {
        format!(r#"<a:solidFill><a:srgbClr val="{:02X}{:02X}{:02X}"><a:alpha val="{}"/></a:srgbClr></a:solidFill>"#,
            c.r, c.g, c.b, u32::from(c.a) * 100_000 / 255)
    })
}

impl Drawing {
    fn id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn emu(&self, value: f32) -> i64 {
        (value * self.scale).round() as i64
    }

    fn transform(&self, rect: Rect) -> String {
        format!(
            r#"<a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
            self.emu(rect.min.x - self.frame.min.x),
            self.emu(rect.min.y - self.frame.min.y),
            self.emu(rect.size.x).max(1),
            self.emu(rect.size.y).max(1)
        )
    }

    fn line(&self, color: Color, width: f32, dashed: bool, arrow: bool) -> String {
        format!(
            r#"<a:ln w="{}">{}<a:prstDash val="{}"/>{}</a:ln>"#,
            self.emu(width).clamp(0, 20_116_800),
            fill(Some(color)),
            if dashed { "dash" } else { "solid" },
            if arrow {
                r#"<a:tailEnd type="triangle"/>"#
            } else {
                ""
            }
        )
    }

    // These private emitters deliberately mirror a DrawingML primitive's attributes.
    #[allow(clippy::too_many_arguments)]
    fn shape(
        &mut self,
        id: u32,
        name: &str,
        rect: Rect,
        geom: &str,
        color: Option<Color>,
        stroke: Color,
        width: f32,
        dashed: bool,
    ) {
        let adjustments = if geom == "capsule" {
            r#"<a:avLst><a:gd name="adj" fmla="val 50000"/></a:avLst>"#
        } else {
            "<a:avLst/>"
        };
        let geom = if geom == "capsule" { "roundRect" } else { geom };
        self.xml.push_str(&format!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst="{geom}">{adjustments}</a:prstGeom>{}{}</p:spPr></p:sp>"#,
            esc(name), self.transform(rect), fill(color), self.line(stroke, width, dashed, false)));
    }

    fn point(&self, point: Vec2) -> String {
        format!(
            r#"<a:pt x="{}" y="{}"/>"#,
            self.emu(point.x - self.frame.min.x),
            self.emu(point.y - self.frame.min.y)
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn path(
        &mut self,
        name: &str,
        points: &[Vec2],
        cubic: bool,
        color: Color,
        width: f32,
        dashed: bool,
        arrow: bool,
    ) {
        let id = self.id();
        let mut path = format!("<a:moveTo>{}</a:moveTo>", self.point(points[0]));
        if cubic {
            path.push_str(&format!(
                "<a:cubicBezTo>{}{}{}</a:cubicBezTo>",
                self.point(points[1]),
                self.point(points[2]),
                self.point(points[3])
            ));
        } else {
            for point in &points[1..] {
                path.push_str(&format!("<a:lnTo>{}</a:lnTo>", self.point(*point)));
            }
        }
        // PowerPoint repairs custom geometry inside p:cxnSp even though the
        // Open XML SDK accepts it. Preserve exact paths as native freeform
        // shapes instead. They remain editable but are not auto-attached to
        // topic connection sites when the exported deck is rearranged.
        self.xml.push_str(&format!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="r" b="b"/><a:pathLst><a:path w="{}" h="{}" fill="none">{path}</a:path></a:pathLst></a:custGeom><a:noFill/>{}</p:spPr></p:sp>"#,
            esc(name), self.width, self.height, self.width, self.height,
            self.line(color, width, dashed, arrow)));
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        name: &str,
        rect: Rect,
        text: &str,
        size: f32,
        family: &str,
        color: Color,
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
    ) {
        let id = self.id();
        let size = (size * self.scale / 127.0).round().clamp(100.0, 400_000.0) as u32;
        let family = esc(family
            .split(',')
            .next()
            .unwrap_or("Segoe UI")
            .trim()
            .trim_matches('"'));
        self.xml.push_str(&format!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap="none" lIns="0" tIns="0" rIns="0" bIns="0" anchor="ctr"><a:noAutofit/></a:bodyPr><a:lstStyle/><a:p><a:pPr algn="ctr"/><a:r><a:rPr lang="en-US" sz="{size}" b="{}" i="{}" u="{}" strike="{}">{}<a:latin typeface="{family}"/><a:ea typeface="{family}"/><a:cs typeface="{family}"/></a:rPr><a:t>{}</a:t></a:r><a:endParaRPr sz="{size}"/></a:p></p:txBody></p:sp>"#,
            esc(name), self.transform(rect), u8::from(bold), u8::from(italic),
            if underline { "sng" } else { "none" }, if strike { "sngStrike" } else { "noStrike" },
            fill(Some(color)), esc(text)));
    }

    fn label(&mut self, rect: Rect, value: &str, size: f32, color: Color) {
        self.text(
            "Label", rect, value, size, "Segoe UI", color, false, false, false, false,
        );
    }
}

pub(crate) fn editable_slide(
    sheet: &Sheet,
    resolved: &ResolvedPresentationSlide,
    options: &ExportOptions,
    slide_number: usize,
    width: u64,
    height: u64,
) -> Result<NativeSlide, IoError> {
    crate::reject_equation_notes(sheet)?;
    let dimensions = thinoda_core::images::validate_visual(sheet).map_err(IoError::Invalid)?;
    let text = ExportText::new(sheet, options.dark, crate::fonts::database())?;
    let layout = &resolved.layout;
    let mut draw = Drawing {
        xml: String::new(),
        next_id: 2,
        frame: resolved.frame,
        scale: width as f32 / resolved.frame.size.x,
        width,
        height,
    };
    let topic_ids: HashMap<TopicId, u32> = layout.order.iter().map(|id| (*id, draw.id())).collect();
    let grey = Color::rgb(0x77, 0x77, 0x77);
    for zone in &resolved.zones {
        let id = draw.id();
        let color = Color::rgb(0x5D, 0x7A, 0x91);
        draw.shape(
            id,
            &zone.name,
            zone.frame,
            "roundRect",
            Some(Color::rgba(color.r, color.g, color.b, 20)),
            color,
            1.0,
            false,
        );
        draw.label(
            Rect::from_min_size(
                zone.frame.min + Vec2::new(8.0, 3.0),
                Vec2::new(zone.frame.size.x - 16.0, 18.0),
            ),
            &zone.name,
            12.0,
            color,
        );
    }
    for boundary in &layout.boundaries {
        let source = sheet
            .boundaries
            .iter()
            .find(|b| b.id == boundary.id)
            .ok_or_else(|| IoError::Invalid("missing native boundary".into()))?;
        let color = source.color.unwrap_or(Color::rgb(0x8A, 0x9B, 0xB0));
        let id = draw.id();
        draw.shape(
            id,
            "Boundary",
            boundary.rect,
            "roundRect",
            Some(Color::rgba(
                color.r,
                color.g,
                color.b,
                (f32::from(color.a) * 0.12).round() as u8,
            )),
            color,
            1.0,
            true,
        );
        if let Some(title) = &source.title {
            draw.label(
                Rect::from_min_size(
                    boundary.rect.min + Vec2::new(6.0, 2.0),
                    Vec2::new(boundary.rect.size.x - 12.0, 18.0),
                ),
                title,
                12.0,
                grey,
            );
        }
    }
    for c in &layout.connectors {
        let points = match c.kind {
            ConnectorKind::Straight => vec![c.start, c.end],
            ConnectorKind::Curve => {
                let delta = c.end - c.start;
                let offset = if c.vertical {
                    Vec2::new(0.0, delta.y * 0.5)
                } else {
                    Vec2::new(delta.x * 0.5, 0.0)
                };
                vec![c.start, c.start + offset, c.end - offset, c.end]
            }
            ConnectorKind::Elbow
            | ConnectorKind::RoundedElbow
            | ConnectorKind::Tree
            | ConnectorKind::Brace => {
                let mid = (c.start + c.end) * 0.5;
                if c.vertical {
                    vec![
                        c.start,
                        Vec2::new(c.start.x, mid.y),
                        Vec2::new(c.end.x, mid.y),
                        c.end,
                    ]
                } else {
                    vec![
                        c.start,
                        Vec2::new(mid.x, c.start.y),
                        Vec2::new(mid.x, c.end.y),
                        c.end,
                    ]
                }
            }
        };
        draw.path(
            "Branch connector",
            &points,
            c.kind == ConnectorKind::Curve,
            c.color,
            c.width,
            false,
            false,
        );
    }
    for relationship in &layout.relationships {
        let source = sheet
            .relationships
            .iter()
            .find(|r| r.id == relationship.id)
            .ok_or_else(|| IoError::Invalid("missing native relationship".into()))?;
        draw.path(
            "Relationship",
            &[
                relationship.start,
                relationship.c1,
                relationship.c2,
                relationship.end,
            ],
            true,
            source.color.unwrap_or(grey),
            1.0,
            true,
            true,
        );
        if !source.title.is_empty() {
            let size = Vec2::new((source.title.chars().count() as f32 * 7.0).max(10.0), 18.0);
            draw.label(
                Rect::from_center_size(relationship.mid, size),
                &source.title,
                12.0,
                grey,
            );
        }
    }
    for summary in &layout.summaries {
        draw.path(
            "Summary bracket",
            &[summary.start, summary.tip, summary.end],
            false,
            grey,
            2.0,
            false,
            false,
        );
    }
    let mut media = Vec::new();
    let mut relationships = String::from(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>"#,
    );
    for id in &layout.order {
        let tl = &layout.topics[id];
        let topic = sheet
            .topic(*id)
            .ok_or_else(|| IoError::Invalid("missing native topic".into()))?;
        let s = &tl.style;
        let stroke = s.border_color.unwrap_or(s.line_color);
        let stroke_width = if s.border_width > 0.0 {
            s.border_width
        } else {
            s.line_width
        };
        let geometry = match s.shape {
            TopicShape::RoundedRect => "roundRect",
            TopicShape::Rect => "rect",
            TopicShape::Ellipse => "ellipse",
            TopicShape::Capsule => "capsule",
            TopicShape::Diamond => "diamond",
            TopicShape::None | TopicShape::Underline => "rect",
        };
        let simple = matches!(s.shape, TopicShape::None | TopicShape::Underline);
        draw.shape(
            topic_ids[id],
            &format!("Topic {id}"),
            tl.rect,
            geometry,
            s.fill,
            if simple { Color::TRANSPARENT } else { stroke },
            stroke_width,
            false,
        );
        if simple {
            draw.path(
                "Topic underline",
                &[Vec2::new(tl.rect.min.x, tl.rect.max().y), tl.rect.max()],
                false,
                stroke,
                stroke_width,
                false,
                false,
            );
        }
        if let (Some(rect), Some(reference)) = (tl.image_rect, topic.image.as_deref()) {
            let resource = sheet.resources.get(reference).map_err(IoError::Invalid)?;
            let extension = match resource.media_type.as_str() {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                other => {
                    return Err(IoError::Invalid(format!(
                        "editable PowerPoint does not support image type {other}"
                    )));
                }
            };
            let size = dimensions
                .get(reference)
                .ok_or_else(|| IoError::Invalid("missing native image dimensions".into()))?;
            let scale = (rect.size.x / size.x).min(rect.size.y / size.y);
            let rect = Rect::from_center_size(rect.center(), *size * scale);
            let image_number = media.len() + 1;
            let relationship_id = image_number + 1;
            let filename = format!("slide{slide_number}-photo{image_number}.{extension}");
            relationships.push_str(&format!(r#"<Relationship Id="rId{relationship_id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/{filename}"/>"#));
            let id = draw.id();
            draw.xml.push_str(&format!(
                r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="Topic photo"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId{relationship_id}"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#,
                draw.transform(rect)));
            media.push((format!("ppt/media/{filename}"), resource.bytes().to_vec()));
        }
        let family = s.font_family.as_deref().unwrap_or("Segoe UI");
        let block = text.topic(*id);
        for (index, line) in block.lines.iter().enumerate() {
            let rect = Rect::from_min_size(
                Vec2::new(
                    tl.title_rect.min.x,
                    tl.title_rect.min.y + index as f32 * block.line_height,
                ),
                Vec2::new(tl.title_rect.size.x, block.line_height),
            );
            draw.text(
                &format!("Topic {id} text line {index}"),
                rect,
                &line.text,
                s.font_size,
                family,
                s.text_color,
                s.bold,
                s.italic,
                s.underline,
                s.strikethrough,
            );
        }
        if let Some((number, rect)) = &tl.number {
            draw.text(
                "Structural number",
                *rect,
                number,
                s.font_size,
                family,
                s.text_color,
                s.bold,
                false,
                false,
                false,
            );
        }
        for (index, (marker, rect)) in tl.markers.iter().enumerate() {
            let color = topic
                .marker_colors
                .get(&index)
                .copied()
                .unwrap_or(s.text_color);
            draw.label(*rect, thinoda_core::markers::glyph(marker), 14.0, color);
        }
        for (label, rect) in &tl.labels {
            let id = draw.id();
            draw.shape(
                id,
                "Label background",
                *rect,
                "roundRect",
                Some(Color::rgb(238, 238, 238)),
                Color::TRANSPARENT,
                0.0,
                false,
            );
            draw.label(*rect, label, 10.0, Color::rgb(51, 51, 51));
        }
        for (icon, rect) in &tl.icons {
            draw.label(
                *rect,
                match icon {
                    Icon::Link => "↗",
                    Icon::Notes => "≡",
                },
                12.0,
                s.text_color,
            );
        }
        if let Some(rect) = tl.todo_rect {
            let id = draw.id();
            draw.shape(
                id,
                "To-do checkbox",
                rect,
                "rect",
                None,
                s.text_color,
                1.0,
                false,
            );
            if topic.todo == Some(true) {
                draw.path(
                    "To-do check",
                    &[
                        rect.min + rect.size * 0.2,
                        Vec2::new(rect.center().x, rect.max().y - rect.size.y * 0.2),
                        rect.max() - rect.size * 0.2,
                    ],
                    false,
                    s.text_color,
                    1.5,
                    false,
                    false,
                );
            }
        }
        if let (Some(progress), Some(rect)) = (tl.todo_progress, tl.todo_rect)
            && progress.nested
            && progress.total > 0
        {
            let id = draw.id();
            let filled = Rect::from_min_size(
                rect.min,
                Vec2::new(
                    rect.size.x * progress.completed as f32 / progress.total as f32,
                    rect.size.y,
                ),
            );
            draw.shape(
                id,
                "To-do progress",
                filled,
                "rect",
                Some(s.text_color),
                Color::TRANSPARENT,
                0.0,
                false,
            );
        }
    }
    relationships.push_str("</Relationships>");
    let background = fill(Some(
        Theme::for_settings(&sheet.theme).background(&sheet.theme, options.dark),
    ));
    Ok(NativeSlide {
        shapes: draw.xml,
        media,
        relationships,
        background,
    })
}
