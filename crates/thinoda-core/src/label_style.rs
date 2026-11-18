//! Shape label text style (draw.io `fontFamily`, `fontSize`, `fontStyle`,
//! `align`, `verticalAlign`, `labelPosition`, `verticalLabelPosition`,
//! `spacing*`, `whiteSpace`, `labelBackgroundColor`, `labelBorderColor` and
//! `horizontal`) and the renderer-neutral label layout the canvas and every
//! exporter share, so line breaks and placement match everywhere.

use crate::board::{CANVAS_LABEL_FONT_SIZE, CanvasObject, rotate_around};
use crate::geometry::{Color, Rect, Vec2};
use crate::model::TextAlign;
use serde::{Deserialize, Serialize};

pub const MIN_LABEL_FONT_SIZE: f32 = 1.0;
pub const MAX_LABEL_FONT_SIZE: f32 = 512.0;
pub const MAX_LABEL_FONT_FAMILY_BYTES: usize = 128;
pub const MAX_LABEL_SPACING: f32 = 1000.0;
/// draw.io's default `spacing`.
pub const DEFAULT_LABEL_SPACING: f32 = 2.0;
/// Estimated advance of one glyph, as a fraction of the font size.
pub const GLYPH_WIDTH: f32 = 0.55;
/// Distance between baselines, as a fraction of the font size.
pub const LINE_HEIGHT: f32 = 1.25;
/// Padding of a label background or border around the text block.
pub const LABEL_BOX_PADDING: f32 = 1.0;

pub const FONT_BOLD: u32 = 1;
pub const FONT_ITALIC: u32 = 2;
pub const FONT_UNDERLINE: u32 = 4;
pub const FONT_STRIKETHROUGH: u32 = 8;

/// Vertical text alignment or label position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LabelVAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// Text style of a shape label. The default is draw.io's default label style,
/// so a shape stores one only when something differs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LabelStyle {
    /// Requested family name; renderers without it use the closest available font.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// `None` uses the board label size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub strikethrough: bool,
    #[serde(skip_serializing_if = "is_center")]
    pub align: TextAlign,
    #[serde(skip_serializing_if = "is_middle")]
    pub vertical_align: LabelVAlign,
    /// Horizontal label position; left and right place the label beside the shape.
    #[serde(skip_serializing_if = "is_center")]
    pub position: TextAlign,
    /// Vertical label position; top and bottom place the label above or below the shape.
    #[serde(skip_serializing_if = "is_middle")]
    pub vertical_position: LabelVAlign,
    #[serde(skip_serializing_if = "is_default_spacing")]
    pub spacing: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub spacing_top: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub spacing_left: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub spacing_bottom: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub spacing_right: f32,
    #[serde(skip_serializing_if = "is_true")]
    pub wrap: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<Color>,
    /// Text turned 90° counter-clockwise (draw.io `horizontal=0`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub vertical: bool,
}

fn is_center(value: &TextAlign) -> bool {
    *value == TextAlign::Center
}

fn is_middle(value: &LabelVAlign) -> bool {
    *value == LabelVAlign::Middle
}

fn is_default_spacing(value: &f32) -> bool {
    *value == DEFAULT_LABEL_SPACING
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn is_true(value: &bool) -> bool {
    *value
}

impl Default for LabelStyle {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: None,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            align: TextAlign::Center,
            vertical_align: LabelVAlign::Middle,
            position: TextAlign::Center,
            vertical_position: LabelVAlign::Middle,
            spacing: DEFAULT_LABEL_SPACING,
            spacing_top: 0.0,
            spacing_left: 0.0,
            spacing_bottom: 0.0,
            spacing_right: 0.0,
            wrap: true,
            background: None,
            border: None,
            vertical: false,
        }
    }
}

impl LabelStyle {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Effective font size in sheet units.
    pub fn size(&self) -> f32 {
        self.font_size.unwrap_or(CANVAS_LABEL_FONT_SIZE)
    }

    /// draw.io `fontStyle` bit mask.
    pub fn font_style_bits(&self) -> u32 {
        [
            (self.bold, FONT_BOLD),
            (self.italic, FONT_ITALIC),
            (self.underline, FONT_UNDERLINE),
            (self.strikethrough, FONT_STRIKETHROUGH),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .fold(0, |bits, (_, bit)| bits | bit)
    }

    pub fn set_font_style_bits(&mut self, bits: u32) {
        self.bold = bits & FONT_BOLD != 0;
        self.italic = bits & FONT_ITALIC != 0;
        self.underline = bits & FONT_UNDERLINE != 0;
        self.strikethrough = bits & FONT_STRIKETHROUGH != 0;
    }

    /// Whether the label sits outside the shape on either axis.
    pub fn is_outside(&self) -> bool {
        self.position != TextAlign::Center || self.vertical_position != LabelVAlign::Middle
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(family) = &self.font_family
            && (family.trim().is_empty()
                || family.trim() != family
                || family.len() > MAX_LABEL_FONT_FAMILY_BYTES
                || family.chars().any(char::is_control))
        {
            return Err("label style has an invalid font family".into());
        }
        if let Some(size) = self.font_size
            && (!size.is_finite() || !(MIN_LABEL_FONT_SIZE..=MAX_LABEL_FONT_SIZE).contains(&size))
        {
            return Err("label style has an invalid font size".into());
        }
        let spacing = [
            self.spacing,
            self.spacing_top,
            self.spacing_left,
            self.spacing_bottom,
            self.spacing_right,
        ];
        if spacing
            .iter()
            .any(|value| !value.is_finite() || value.abs() > MAX_LABEL_SPACING)
        {
            return Err("label style has invalid spacing".into());
        }
        if self.is_default() {
            return Err("label style repeats the default style".into());
        }
        Ok(())
    }
}

/// Greedy word wrap with the shared glyph estimate; explicit newlines are kept.
pub fn wrap_lines(text: &str, width: f32, size: f32) -> Vec<String> {
    let max_chars = ((width / (size * GLYPH_WIDTH)).floor() as usize).max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word.to_owned();
            while word.chars().count() > max_chars {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let split = word
                    .char_indices()
                    .nth(max_chars)
                    .map_or(word.len(), |(index, _)| index);
                lines.push(word[..split].to_owned());
                word = word[split..].to_owned();
            }
            if line.is_empty() {
                line = word;
            } else if line.chars().count() + 1 + word.chars().count() <= max_chars {
                line.push(' ');
                line.push_str(&word);
            } else {
                lines.push(std::mem::replace(&mut line, word));
            }
        }
        lines.push(line);
    }
    while lines.last().is_some_and(String::is_empty) && lines.len() > 1 {
        lines.pop();
    }
    lines
}

/// One laid-out label line; `rect` is its estimated box, anchored by the alignment.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelLine {
    pub text: String,
    pub rect: Rect,
}

/// A styled label in text space: unrotated coordinates turned by `rotation`
/// degrees clockwise around `pivot`.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelLayout {
    /// Text area after position and spacing.
    pub frame: Rect,
    /// Estimated bounds of all lines.
    pub block: Rect,
    pub lines: Vec<LabelLine>,
    pub size: f32,
    pub line_height: f32,
    pub align: TextAlign,
    pub pivot: Vec2,
    pub rotation: f32,
}

impl LabelLayout {
    /// Background and border box around the text block.
    pub fn box_rect(&self) -> Rect {
        self.block.expand(LABEL_BOX_PADDING)
    }

    /// Axis-aligned sheet bounds of the frame and text.
    pub fn bounds(&self) -> Rect {
        let rect = union(self.frame, self.box_rect());
        let corners = [
            rect.min,
            Vec2::new(rect.max().x, rect.min.y),
            rect.max(),
            Vec2::new(rect.min.x, rect.max().y),
        ]
        .map(|point| rotate_around(point, self.pivot, self.rotation));
        let min = corners
            .iter()
            .fold(Vec2::new(f32::INFINITY, f32::INFINITY), |acc, p| {
                Vec2::new(acc.x.min(p.x), acc.y.min(p.y))
            });
        let max = corners
            .iter()
            .fold(Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY), |acc, p| {
                Vec2::new(acc.x.max(p.x), acc.y.max(p.y))
            });
        Rect::from_min_size(min, max - min)
    }

    /// Whether `point` (sheet coordinates) lies on the text block.
    pub fn hits(&self, point: Vec2) -> bool {
        !self.lines.is_empty()
            && self
                .box_rect()
                .contains(rotate_around(point, self.pivot, -self.rotation))
    }

    /// Sheet position of a text-space point.
    pub fn to_sheet(&self, point: Vec2) -> Vec2 {
        rotate_around(point, self.pivot, self.rotation)
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    let min = Vec2::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y));
    let max = Vec2::new(a.max().x.max(b.max().x), a.max().y.max(b.max().y));
    Rect::from_min_size(min, max - min)
}

/// Layout of `text` for a styled shape label whose default text area is `base`.
pub fn layout(object: &CanvasObject, base: Rect, text: &str) -> Option<LabelLayout> {
    let style = object.label_style.as_deref()?;
    Some(layout_with(style, object.rect, object.rotation, base, text))
}

/// Layout of `text` in `base` for a shape frame `rect` turned by `rotation`.
pub fn layout_with(
    style: &LabelStyle,
    rect: Rect,
    rotation: f32,
    base: Rect,
    text: &str,
) -> LabelLayout {
    let (x, width) = match style.position {
        TextAlign::Center => (base.min.x, base.size.x),
        TextAlign::Left => (rect.min.x - rect.size.x, rect.size.x),
        TextAlign::Right => (rect.max().x, rect.size.x),
    };
    let (y, height) = match style.vertical_position {
        LabelVAlign::Middle => (base.min.y, base.size.y),
        LabelVAlign::Top => (rect.min.y - rect.size.y, rect.size.y),
        LabelVAlign::Bottom => (rect.max().y, rect.size.y),
    };
    let left = x + style.spacing + style.spacing_left;
    let right = x + width - style.spacing - style.spacing_right;
    let top = y + style.spacing + style.spacing_top;
    let bottom = y + height - style.spacing - style.spacing_bottom;
    let center = Vec2::new((left + right) / 2.0, (top + bottom) / 2.0);
    let inner = Vec2::new((right - left).max(0.0), (bottom - top).max(0.0));
    let (frame_size, local) = if style.vertical {
        (Vec2::new(inner.y, inner.x), -90.0)
    } else {
        (inner, 0.0)
    };
    let pivot = rotate_around(center, rect.center(), rotation);
    let frame = Rect::from_center_size(pivot, frame_size);
    let size = style.size();
    let line_height = size * LINE_HEIGHT;
    let texts: Vec<String> = if text.is_empty() {
        Vec::new()
    } else if style.wrap {
        wrap_lines(text, frame.size.x, size)
    } else {
        text.split('\n').map(str::to_owned).collect()
    };
    let height = texts.len() as f32 * line_height;
    let top = match style.vertical_align {
        LabelVAlign::Top => frame.min.y,
        LabelVAlign::Middle => frame.center().y - height / 2.0,
        LabelVAlign::Bottom => frame.max().y - height,
    };
    let lines: Vec<LabelLine> = texts
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let width = text.chars().count() as f32 * size * GLYPH_WIDTH;
            let left = match style.align {
                TextAlign::Left => frame.min.x,
                TextAlign::Center => frame.center().x - width / 2.0,
                TextAlign::Right => frame.max().x - width,
            };
            LabelLine {
                text,
                rect: Rect::from_min_size(
                    Vec2::new(left, top + index as f32 * line_height),
                    Vec2::new(width, line_height),
                ),
            }
        })
        .collect();
    let block = lines
        .iter()
        .map(|line| line.rect)
        .reduce(union)
        .unwrap_or(Rect::from_center_size(frame.center(), Vec2::ZERO));
    LabelLayout {
        frame,
        block,
        lines,
        size,
        line_height,
        align: style.align,
        pivot,
        rotation: crate::board::normalize_rotation(rotation + local),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::ShapeKind;

    fn styled(style: LabelStyle) -> CanvasObject {
        let mut object = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 40.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
            *label = "Hello world".into();
        }
        object.label_style = Some(std::sync::Arc::new(style));
        object
    }

    use crate::board::CanvasObjectKind;

    #[test]
    fn font_style_bits_round_trip() {
        let mut style = LabelStyle::default();
        style.set_font_style_bits(FONT_BOLD | FONT_STRIKETHROUGH);
        assert!(style.bold && style.strikethrough && !style.italic && !style.underline);
        assert_eq!(style.font_style_bits(), 9);
    }

    #[test]
    fn validation_rejects_default_and_out_of_range_values() {
        assert!(LabelStyle::default().validate().is_err());
        for style in [
            LabelStyle {
                font_size: Some(0.0),
                ..LabelStyle::default()
            },
            LabelStyle {
                font_family: Some(" Arial".into()),
                ..LabelStyle::default()
            },
            LabelStyle {
                spacing_left: f32::NAN,
                ..LabelStyle::default()
            },
        ] {
            assert!(style.validate().is_err(), "{style:?}");
        }
        assert!(
            LabelStyle {
                bold: true,
                ..LabelStyle::default()
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn alignment_spacing_and_outside_position_place_lines() {
        let object = styled(LabelStyle {
            align: TextAlign::Left,
            vertical_align: LabelVAlign::Top,
            spacing: 0.0,
            spacing_left: 5.0,
            spacing_top: 3.0,
            ..LabelStyle::default()
        });
        let placed = super::layout(&object, object.rect, object.label()).unwrap();
        assert_eq!(placed.lines[0].rect.min, Vec2::new(5.0, 3.0));

        let object = styled(LabelStyle {
            vertical_position: LabelVAlign::Bottom,
            vertical_align: LabelVAlign::Top,
            ..LabelStyle::default()
        });
        let placed = super::layout(&object, object.rect, object.label()).unwrap();
        assert!(placed.block.min.y >= 40.0);
        assert!(placed.hits(placed.block.center()));
        assert!(!placed.hits(object.rect.center()));

        let object = styled(LabelStyle {
            position: TextAlign::Right,
            align: TextAlign::Left,
            ..LabelStyle::default()
        });
        let placed = super::layout(&object, object.rect, object.label()).unwrap();
        assert_eq!(placed.block.min.x, 102.0);
    }

    #[test]
    fn wrap_and_vertical_text_use_the_shared_estimate() {
        let wrapped = styled(LabelStyle {
            font_size: Some(20.0),
            ..LabelStyle::default()
        });
        let layout_wrapped = super::layout(&wrapped, wrapped.rect, wrapped.label()).unwrap();
        assert_eq!(layout_wrapped.lines.len(), 2);
        let single = styled(LabelStyle {
            font_size: Some(20.0),
            wrap: false,
            ..LabelStyle::default()
        });
        assert_eq!(
            super::layout(&single, single.rect, single.label())
                .unwrap()
                .lines
                .len(),
            1
        );
        let vertical = styled(LabelStyle {
            vertical: true,
            ..LabelStyle::default()
        });
        let placed = super::layout(&vertical, vertical.rect, vertical.label()).unwrap();
        assert_eq!(placed.rotation, 270.0);
        assert_eq!(placed.frame.size, Vec2::new(36.0, 96.0));
        assert_eq!(placed.pivot, Vec2::new(50.0, 20.0));
    }
}
