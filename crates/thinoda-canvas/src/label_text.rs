//! Paints styled shape labels from the shared core layout so the canvas uses
//! the same line breaks and placement as every exporter.

use egui::{
    Color32, FontId, Painter, Pos2, Stroke,
    emath::Rot2,
    epaint::{PathShape, TextShape},
    text::{LayoutJob, TextFormat},
    vec2,
};
use thinoda_core::{
    geometry::Vec2,
    label_style::{LabelLayout, LabelStyle, run_baseline_shift, run_font_size},
    model::TextAlign,
};

use crate::render::{Xf, color32, opacity_color};

/// Screen-space corners of a text-space rectangle.
fn corners(layout: &LabelLayout, rect: thinoda_core::Rect, xf: Xf) -> Vec<Pos2> {
    let max = rect.max();
    [
        rect.min,
        Vec2::new(max.x, rect.min.y),
        max,
        Vec2::new(rect.min.x, max.y),
    ]
    .into_iter()
    .map(|point| xf.pos(layout.to_sheet(point)))
    .collect()
}

pub(crate) fn paint(
    painter: &Painter,
    layout: &LabelLayout,
    style: &LabelStyle,
    color: Color32,
    opacity: f32,
    xf: Xf,
) {
    if layout.lines.is_empty() {
        return;
    }
    let z = xf.zoom;
    if style.background.is_some() || style.border.is_some() {
        let fill = style.background.map_or(Color32::TRANSPARENT, |fill| {
            opacity_color(color32(fill), opacity)
        });
        let stroke = style.border.map_or(Stroke::NONE, |border| {
            Stroke::new(z.max(0.5), opacity_color(color32(border), opacity))
        });
        painter.add(PathShape::convex_polygon(
            corners(layout, layout.box_rect(), xf),
            fill,
            stroke,
        ));
    }
    let angle = layout.rotation.to_radians();
    let rotation = Rot2::from_angle(angle);
    let pivot = xf.pos(layout.pivot);
    let font = FontId::new(
        crate::measure::render_font_size(layout.size * z),
        crate::measure::font_family(painter.ctx(), style.font_family.as_deref()),
    );
    let decoration = Stroke::new((layout.size * z / 14.0).max(0.6), color);
    for line in &layout.lines {
        if line.text.is_empty() {
            continue;
        }
        if !line.runs.is_empty() {
            let mut runs = Vec::with_capacity(line.runs.len());
            for run in &line.runs {
                let run_color = run
                    .style
                    .color
                    .map_or(color, |value| opacity_color(color32(value), opacity));
                let run_size = run_font_size(layout.size, &run.style);
                let family = run
                    .style
                    .font_family
                    .as_deref()
                    .or(style.font_family.as_deref());
                let decoration = Stroke::new((run_size * z / 14.0).max(0.6), run_color);
                let mut job = LayoutJob::default();
                job.append(
                    &run.text,
                    0.0,
                    TextFormat {
                        font_id: FontId::new(
                            crate::measure::render_font_size(run_size * z),
                            crate::measure::font_family(painter.ctx(), family),
                        ),
                        color: run_color,
                        italics: run.style.italic.unwrap_or(style.italic),
                        underline: if run.style.underline.unwrap_or(style.underline) {
                            decoration
                        } else {
                            Stroke::NONE
                        },
                        strikethrough: if run.style.strikethrough.unwrap_or(style.strikethrough) {
                            decoration
                        } else {
                            Stroke::NONE
                        },
                        ..Default::default()
                    },
                );
                runs.push((run, painter.layout_job(job), run_color));
            }
            let rect = xf.rect(line.rect);
            let width = runs
                .iter()
                .map(|(_, galley, _)| galley.size().x)
                .sum::<f32>();
            let mut x = match line.align {
                TextAlign::Left => rect.left(),
                TextAlign::Center => rect.center().x - width / 2.0,
                TextAlign::Right => rect.right() - width,
            };
            for (run, galley, run_color) in runs {
                let unrotated = Pos2::new(
                    x,
                    rect.center().y - galley.size().y / 2.0
                        + run_baseline_shift(layout.size, &run.style) * z,
                );
                let origin = pivot + rotation * (unrotated - pivot);
                if run.style.bold.unwrap_or(style.bold) {
                    painter.add(
                        TextShape::new(
                            origin + rotation * vec2(0.45 * z, 0.0),
                            galley.clone(),
                            run_color,
                        )
                        .with_angle(angle),
                    );
                }
                x += galley.size().x;
                painter.add(TextShape::new(origin, galley, run_color).with_angle(angle));
            }
            continue;
        }
        let mut job = LayoutJob::default();
        job.append(
            &line.text,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color,
                italics: style.italic,
                underline: if style.underline {
                    decoration
                } else {
                    Stroke::NONE
                },
                strikethrough: if style.strikethrough {
                    decoration
                } else {
                    Stroke::NONE
                },
                ..Default::default()
            },
        );
        let galley = painter.layout_job(job);
        let size = galley.size();
        let rect = xf.rect(line.rect);
        let x = match line.align {
            TextAlign::Left => rect.left(),
            TextAlign::Center => rect.center().x - size.x / 2.0,
            TextAlign::Right => rect.right() - size.x,
        };
        let unrotated = Pos2::new(x, rect.center().y - size.y / 2.0);
        let origin = pivot + rotation * (unrotated - pivot);
        if style.bold {
            painter.add(
                TextShape::new(
                    origin + rotation * vec2(0.45 * z, 0.0),
                    galley.clone(),
                    color,
                )
                .with_angle(angle),
            );
        }
        painter.add(TextShape::new(origin, galley, color).with_angle(angle));
    }
}

pub(crate) fn paint_connector(
    painter: &Painter,
    layout: &LabelLayout,
    color: Color32,
    background: Color32,
    opacity: f32,
    xf: Xf,
) {
    let style = LabelStyle::default();
    let mut bounds = xf.rect(layout.block);
    for line in &layout.lines {
        let mut runs = Vec::with_capacity(line.runs.len());
        for run in &line.runs {
            let size = run_font_size(layout.size, &run.style);
            let font = FontId::new(
                crate::measure::render_font_size(size * xf.zoom),
                crate::measure::font_family(painter.ctx(), run.style.font_family.as_deref()),
            );
            let galley = painter.layout_no_wrap(run.text.clone(), font, color);
            runs.push((run, galley));
        }
        let width = runs.iter().map(|(_, galley)| galley.size().x).sum::<f32>();
        let rect = xf.rect(line.rect);
        let mut x = match line.align {
            TextAlign::Left => rect.left(),
            TextAlign::Center => rect.center().x - width / 2.0,
            TextAlign::Right => rect.right() - width,
        };
        for (run, galley) in runs {
            let y = xf.rect(line.rect).center().y - galley.size().y / 2.0
                + run_baseline_shift(layout.size, &run.style) * xf.zoom;
            bounds = bounds.union(egui::Rect::from_min_size(Pos2::new(x, y), galley.size()));
            x += galley.size().x;
        }
    }
    painter.rect_filled(
        bounds.expand2(vec2(4.0, 2.0) * xf.zoom),
        4.0 * xf.zoom,
        background,
    );
    paint(painter, layout, &style, color, opacity, xf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use thinoda_core::{
        CanvasObject, Rect, ShapeKind,
        label_style::LabelStyle,
        rich_text::{InlineText, InlineTextRun, InlineTextStyle},
    };

    #[test]
    fn rich_runs_use_measured_advances_and_preserve_alignment() {
        let text = "WWWWiiiiWWWW";
        for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
            let mut object = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 100.0)),
            );
            object.label_style = Some(Arc::new(LabelStyle {
                font_size: Some(20.0),
                align,
                ..Default::default()
            }));
            object.label_rich_text = Some(Arc::new(InlineText {
                runs: vec![InlineTextRun {
                    start: 4,
                    end: 8,
                    style: InlineTextStyle {
                        italic: Some(true),
                        ..Default::default()
                    },
                }],
                ..Default::default()
            }));
            let layout = thinoda_core::label_style::layout(&object, object.rect, text).unwrap();
            let ctx = egui::Context::default();
            let mut output = ctx.run_pass(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(400.0, 200.0))),
                    ..Default::default()
                },
                |ctx| {
                    paint(
                        &ctx.layer_painter(egui::LayerId::background()),
                        &layout,
                        object.label_style.as_deref().unwrap(),
                        Color32::BLACK,
                        1.0,
                        Xf {
                            origin: Pos2::ZERO,
                            zoom: 1.0,
                        },
                    );
                },
            );
            output.textures_delta.clear();
            let runs: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text),
                    _ => None,
                })
                .collect();
            assert_eq!(runs.len(), 3);
            for pair in runs.windows(2) {
                let end = pair[0].pos.x + pair[0].galley.size().x;
                assert!((end - pair[1].pos.x).abs() < 0.01, "{align:?}");
            }
            let start = runs[0].pos.x;
            let end = runs[2].pos.x + runs[2].galley.size().x;
            let rect = layout.lines[0].rect;
            let expected = match align {
                TextAlign::Left => (start, rect.min.x),
                TextAlign::Center => ((start + end) / 2.0, rect.center().x),
                TextAlign::Right => (end, rect.max().x),
            };
            assert!((expected.0 - expected.1).abs() < 0.01, "{align:?}");
        }
    }

    #[test]
    fn rich_card_layout_paints_inline_style_and_list_markers() {
        let text = "First\nSecond";
        let mut rich = InlineText {
            paragraphs: vec![
                thinoda_core::InlineParagraph {
                    index: 0,
                    list: thinoda_core::ListKind::Bulleted,
                },
                thinoda_core::InlineParagraph {
                    index: 1,
                    list: thinoda_core::ListKind::Numbered,
                },
            ],
            ..Default::default()
        };
        rich.update_range(text, 6..12, |style| {
            style.italic = Some(true);
            style.font_size = Some(24.0);
            style.color = Some(thinoda_core::Color::rgb(20, 80, 160));
        })
        .unwrap();
        let style = LabelStyle {
            font_size: Some(14.0),
            align: TextAlign::Left,
            spacing: 0.0,
            ..Default::default()
        };
        let frame = Rect::from_min_size(Vec2::ZERO, Vec2::new(220.0, 100.0));
        let layout = thinoda_core::label_style::layout_rich_in_frame(&style, frame, text, &rich);
        let ctx = egui::Context::default();
        let mut output = ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(400.0, 200.0))),
                ..Default::default()
            },
            |ctx| {
                paint(
                    &ctx.layer_painter(egui::LayerId::background()),
                    &layout,
                    &style,
                    Color32::BLACK,
                    1.0,
                    Xf {
                        origin: Pos2::ZERO,
                        zoom: 1.0,
                    },
                );
            },
        );
        output.textures_delta.clear();
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|text| text.contains('•')), "{texts:?}");
        assert!(texts.iter().any(|text| text.contains("1.")), "{texts:?}");
        assert!(texts.iter().any(|text| text.contains("First")), "{texts:?}");
        let second = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == "Second" => Some(text),
            _ => None,
        });
        let second = second.expect("formatted second paragraph");
        assert!(second.galley.job.sections[0].format.italics);
        assert_eq!(
            second.galley.job.sections[0].format.color,
            color32(thinoda_core::Color::rgb(20, 80, 160))
        );
        assert!(second.galley.job.sections[0].format.font_id.size > 20.0);
    }

    #[test]
    fn connector_list_items_share_the_same_canvas_left_edge() {
        let text = "Short\nMuch longer";
        let rich = InlineText {
            paragraphs: (0..2)
                .map(|index| thinoda_core::InlineParagraph {
                    index,
                    list: thinoda_core::ListKind::Bulleted,
                })
                .collect(),
            ..Default::default()
        };
        let layout = thinoda_core::label_style::layout_connector(
            Vec2::new(200.0, 100.0),
            text,
            &rich,
            false,
        );
        let ctx = egui::Context::default();
        let mut output = ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(400.0, 200.0))),
                ..Default::default()
            },
            |ctx| {
                paint_connector(
                    &ctx.layer_painter(egui::LayerId::background()),
                    &layout,
                    Color32::BLACK,
                    Color32::WHITE,
                    1.0,
                    Xf {
                        origin: Pos2::ZERO,
                        zoom: 1.0,
                    },
                );
            },
        );
        output.textures_delta.clear();
        let lines: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 2);
        assert!((lines[0].pos.x - lines[1].pos.x).abs() < 0.01);
    }
}
