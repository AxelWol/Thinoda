//! Original topic appearance presets and portable named quick styles.

use crate::{Color, LineShape, TextAlign, TopicShape, TopicStyle, theme::ResolvedStyle};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickStyle {
    pub name: String,
    pub style: TopicStyle,
}

pub(crate) fn validate_named_style(
    name: &str,
    style: &TopicStyle,
    kind: &str,
) -> Result<(), String> {
    if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(format!(
            "{kind} name must contain 1-80 printable characters"
        ));
    }
    for (name, value, min, max) in [
        ("font size", style.font_size, 6.0, 96.0),
        ("border width", style.border_width, 0.0, 16.0),
        ("line width", style.line_width, 0.0, 16.0),
        ("topic width", style.width, 1.0, 10000.0),
    ] {
        if value.is_some_and(|value| !value.is_finite() || !(min..=max).contains(&value)) {
            return Err(format!("{kind} {name} must be between {min} and {max}"));
        }
    }
    Ok(())
}

impl QuickStyle {
    pub fn validate(&self) -> Result<(), String> {
        validate_named_style(&self.name, &self.style, "quick style")
    }

    /// Freeze resolved colors/typography, retaining automatic wrapping when no width is set.
    pub fn from_appearance(name: String, r: &ResolvedStyle) -> Self {
        Self {
            name,
            style: TopicStyle {
                fill: Some(r.fill.unwrap_or(Color::TRANSPARENT)),
                text_color: Some(r.text_color),
                border_color: Some(r.border_color.unwrap_or(Color::TRANSPARENT)),
                border_width: Some(if r.border_color.is_some() {
                    r.border_width
                } else {
                    0.0
                }),
                shape: Some(r.shape),
                font_family: Some(r.font_family.clone().unwrap_or_else(|| "sans-serif".into())),
                font_size: Some(r.font_size),
                bold: Some(r.bold),
                italic: Some(r.italic),
                underline: Some(r.underline),
                strikethrough: Some(r.strikethrough),
                align: Some(r.align),
                line_color: Some(r.line_color),
                line_shape: Some(r.line_shape),
                line_width: Some(r.line_width),
                width: r.width,
            },
        }
    }
}

pub const PRESETS: [&str; 4] = ["Emphasis", "Note", "Warning", "Muted"];

pub fn preset(name: &str) -> Option<QuickStyle> {
    let (fill, text, bold, italic) = match name {
        "Emphasis" => (Color::rgb(46, 124, 246), Color::WHITE, true, false),
        "Note" => (
            Color::rgb(224, 242, 254),
            Color::rgb(12, 74, 110),
            false,
            false,
        ),
        "Warning" => (
            Color::rgb(255, 237, 213),
            Color::rgb(124, 45, 18),
            true,
            false,
        ),
        "Muted" => (
            Color::rgb(229, 231, 235),
            Color::rgb(55, 65, 81),
            false,
            true,
        ),
        _ => return None,
    };
    Some(QuickStyle {
        name: name.into(),
        style: TopicStyle {
            fill: Some(fill),
            text_color: Some(text),
            border_color: Some(fill),
            border_width: Some(0.0),
            shape: Some(TopicShape::RoundedRect),
            font_size: Some(16.0),
            font_family: Some("sans-serif".into()),
            bold: Some(bold),
            italic: Some(italic),
            underline: Some(false),
            strikethrough: Some(false),
            align: Some(TextAlign::Center),
            line_color: Some(fill),
            line_shape: Some(LineShape::Curve),
            line_width: Some(1.5),
            ..Default::default()
        },
    })
}
