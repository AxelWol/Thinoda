//! Map themes and style resolution.
//!
//! A [`Theme`] provides default colors per topic level; per-topic [`TopicStyle`]
//! overrides win. [`ThemeSettings`] is stored per sheet (Format panel → "Map" tab).

use crate::geometry::Color;
use crate::model::{LineShape, Sheet, TextAlign, TopicId, TopicShape};
use serde::{Deserialize, Serialize};

/// Per-sheet theme configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    /// Theme id, see [`Theme::builtin`].
    pub theme_id: String,
    /// Color variant (0 = multicolor, 1.. = single palette color).
    pub variant: usize,
    /// Explicit map background; `None` follows the app light/dark mode.
    pub background: Option<Color>,
    /// Color each main branch differently ("rainbow branches").
    pub rainbow: bool,
    /// Use the subtree heights to balance left/right in map structures.
    pub balanced: bool,
    /// Taper branch lines (thicker near the center).
    pub tapered_lines: bool,
    /// Override global font family.
    pub font_family: Option<String>,
    /// Spacing multiplier between topics.
    pub spacing: f32,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            theme_id: "twilight".into(),
            variant: 0,
            background: None,
            rainbow: true,
            balanced: false,
            tapered_lines: true,
            font_family: None,
            spacing: 1.0,
        }
    }
}

/// A built-in theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub palette: Vec<Color>,
    pub central_fill: Color,
    pub central_text: Color,
    pub light_bg: Color,
    pub dark_bg: Color,
    pub dark_only: bool,
    pub central_shape: TopicShape,
    pub main_shape: TopicShape,
    pub sub_shape: TopicShape,
    pub line_shape: LineShape,
}

fn c(hex: &str) -> Color {
    Color::from_hex(hex).expect("valid builtin color")
}

impl Theme {
    pub const IDS: [&'static str; 8] = [
        "clarity",
        "twilight",
        "ocean",
        "forest",
        "classic",
        "monochrome",
        "neon",
        "pastel",
    ];

    pub fn builtin(id: &str) -> Theme {
        let base = |id, name, pal: &[&str], central: &str| Theme {
            id,
            name,
            palette: pal.iter().map(|h| c(h)).collect(),
            central_fill: c(central),
            central_text: c(central).contrasting_text(),
            light_bg: c("#FFFFFF"),
            dark_bg: c("#1E1E1E"),
            dark_only: false,
            central_shape: TopicShape::RoundedRect,
            main_shape: TopicShape::RoundedRect,
            sub_shape: TopicShape::Underline,
            line_shape: LineShape::Curve,
        };
        match id {
            "clarity" => Theme {
                central_shape: TopicShape::None,
                ..base(
                    "clarity",
                    "Clarity",
                    &[
                        "#FF656B", "#FFA366", "#98D3B8", "#87E0D8", "#6EC4F5", "#D58BF0",
                    ],
                    "#FFFFFF",
                )
            },
            "ocean" => base(
                "ocean",
                "Ocean",
                &[
                    "#0077B6", "#00B4D8", "#48CAE4", "#0096C7", "#023E8A", "#90E0EF",
                ],
                "#03045E",
            ),
            "forest" => base(
                "forest",
                "Forest",
                &[
                    "#2D6A4F", "#40916C", "#52B788", "#74C69D", "#95D5B2", "#1B4332",
                ],
                "#081C15",
            ),
            "classic" => Theme {
                sub_shape: TopicShape::RoundedRect,
                line_shape: LineShape::RoundedElbow,
                ..base(
                    "classic",
                    "Classic",
                    &[
                        "#4A90D9", "#4A90D9", "#4A90D9", "#4A90D9", "#4A90D9", "#4A90D9",
                    ],
                    "#1F4E79",
                )
            },
            "monochrome" => base(
                "monochrome",
                "Monochrome",
                &[
                    "#333333", "#555555", "#777777", "#999999", "#444444", "#666666",
                ],
                "#111111",
            ),
            "neon" => Theme {
                dark_only: true,
                light_bg: c("#101018"),
                dark_bg: c("#101018"),
                ..base(
                    "neon",
                    "Neon",
                    &[
                        "#FF2E88", "#00F0FF", "#B4FF00", "#FFD000", "#9D4EDD", "#FF7A00",
                    ],
                    "#FF2E88",
                )
            },
            "pastel" => base(
                "pastel",
                "Pastel",
                &[
                    "#FFADAD", "#FFD6A5", "#FDFFB6", "#CAFFBF", "#9BF6FF", "#BDB2FF",
                ],
                "#6D6875",
            ),
            _ => base(
                "twilight",
                "Twilight",
                &[
                    "#FF6B6B", "#FFA26B", "#94D2B5", "#86DFD7", "#6EC4F5", "#D58BF0",
                ],
                "#2E3440",
            ),
        }
    }

    /// Number of selectable color variants (multicolor + one per palette color).
    pub fn variant_count(&self) -> usize {
        1 + self.palette.len().min(5)
    }

    pub fn background(&self, settings: &ThemeSettings, dark: bool) -> Color {
        settings.background.unwrap_or(if dark || self.dark_only {
            self.dark_bg
        } else {
            self.light_bg
        })
    }

    /// Branch color for main branch `index`.
    pub fn branch_color(&self, settings: &ThemeSettings, index: usize) -> Color {
        let n = self.palette.len().max(1);
        if settings.variant > 0 {
            self.palette[(settings.variant - 1) % n]
        } else if settings.rainbow {
            self.palette[index % n]
        } else {
            self.palette[0]
        }
    }
}

/// Fully resolved visual style for a topic.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedStyle {
    pub fill: Option<Color>,
    pub text_color: Color,
    pub border_color: Option<Color>,
    pub border_width: f32,
    pub shape: TopicShape,
    pub font_family: Option<String>,
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub align: TextAlign,
    pub line_color: Color,
    pub line_shape: LineShape,
    pub line_width: f32,
    pub width: Option<f32>,
}

/// Topic level used for default styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Central,
    Main,
    Sub,
    Floating,
    Summary,
}

pub fn level_of(sheet: &Sheet, id: TopicId) -> Level {
    let top = sheet.top_of(id);
    let depth = sheet.depth(id);
    if top == sheet.root {
        match depth {
            0 => Level::Central,
            1 => Level::Main,
            _ => Level::Sub,
        }
    } else if sheet.summary_for_topic(top).is_some() && depth == 0 {
        Level::Summary
    } else if depth == 0 {
        Level::Floating
    } else {
        Level::Sub
    }
}

/// Resolves the visual style of `id` on `sheet` in light or dark mode.
pub fn resolve_style(sheet: &Sheet, id: TopicId, dark: bool) -> ResolvedStyle {
    let theme = Theme::builtin(&sheet.theme.theme_id);
    let settings = &sheet.theme;
    let bg = theme.background(settings, dark);
    let level = level_of(sheet, id);
    let branch_index = sheet
        .main_branch_of(id)
        .and_then(|m| sheet.index_in_parent(m))
        .unwrap_or(0);
    let branch = theme.branch_color(settings, branch_index);
    let topic = sheet.topic(id);
    let clarity = theme.id == "clarity";
    let depth = sheet.depth(id);

    let (fill, shape, size, bold) = match level {
        Level::Central if clarity => (None, theme.central_shape, 30.0, true),
        Level::Sub if clarity && depth == 2 => (
            Some(branch.lerp(bg, if dark { 0.65 } else { 0.8 })),
            TopicShape::RoundedRect,
            14.0,
            false,
        ),
        Level::Central => (Some(theme.central_fill), theme.central_shape, 30.0, true),
        Level::Main => (Some(branch), theme.main_shape, 18.0, false),
        Level::Floating | Level::Summary => (
            Some(branch.lerp(bg, 0.7)),
            TopicShape::RoundedRect,
            16.0,
            false,
        ),
        Level::Sub => (None, theme.sub_shape, 14.0, false),
    };
    // inherit the line color from the nearest styled ancestor
    let mut line_color = if level == Level::Central {
        theme.central_fill
    } else {
        branch
    };
    for a in std::iter::once(id).chain(sheet.ancestors(id)) {
        if let Some(lc) = sheet.topic(a).and_then(|t| t.style.line_color) {
            line_color = lc;
            break;
        }
    }
    let mut line_shape = if clarity && level == Level::Sub {
        LineShape::RoundedElbow
    } else {
        theme.line_shape
    };
    for a in std::iter::once(id).chain(sheet.ancestors(id)) {
        if let Some(ls) = sheet.topic(a).and_then(|t| t.style.line_shape) {
            line_shape = ls;
            break;
        }
    }

    let mut r = ResolvedStyle {
        fill,
        text_color: Color::BLACK,
        border_color: None,
        border_width: 2.0,
        shape,
        font_family: settings.font_family.clone(),
        font_size: size,
        bold,
        italic: false,
        underline: false,
        strikethrough: false,
        align: TextAlign::Center,
        line_color,
        line_shape,
        line_width: match level {
            Level::Central => 3.0,
            Level::Main => 2.5,
            _ => 1.5,
        },
        width: None,
    };
    if level == Level::Sub && !(clarity && depth == 2) {
        r.border_color = Some(branch);
    }
    if clarity {
        r.line_width = 1.5;
    }
    if let Some(t) = topic {
        let s = &t.style;
        if s.fill.is_some() {
            r.fill = s.fill;
        }
        if let Some(v) = s.border_color {
            r.border_color = Some(v);
        }
        if let Some(v) = s.border_width {
            r.border_width = v;
        }
        if let Some(v) = s.shape {
            r.shape = v;
        }
        if let Some(v) = &s.font_family {
            r.font_family = Some(v.clone());
        }
        if let Some(v) = s.font_size {
            r.font_size = v;
        }
        r.bold = s.bold.unwrap_or(r.bold);
        r.italic = s.italic.unwrap_or(r.italic);
        r.underline = s.underline.unwrap_or(r.underline);
        r.strikethrough = s.strikethrough.unwrap_or(r.strikethrough);
        r.align = s.align.unwrap_or(r.align);
        if let Some(v) = s.line_width {
            r.line_width = v;
        }
        r.width = s.width;
    }
    let behind = r.fill.unwrap_or(bg);
    r.text_color = topic.and_then(|t| t.style.text_color).unwrap_or_else(|| {
        if clarity && level == Level::Main && topic.is_none_or(|t| t.style.fill.is_none()) {
            c("#1F2328")
        } else {
            behind.contrasting_text()
        }
    });
    if level == Level::Central && !clarity {
        r.text_color = topic
            .and_then(|t| t.style.text_color)
            .unwrap_or(theme.central_text);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Sheet;

    #[test]
    fn builtin_themes_exist() {
        for id in Theme::IDS {
            let t = Theme::builtin(id);
            assert_eq!(t.id, id);
            assert!(!t.palette.is_empty());
        }
        assert_eq!(Theme::builtin("nope").id, "twilight");
    }

    #[test]
    fn levels_and_defaults() {
        let mut s = Sheet::new("S", "R");
        let m = s.add_child(s.root, None, "M");
        let sub = s.add_child(m, None, "S");
        let f = s.add_floating("F", Default::default());
        assert_eq!(level_of(&s, s.root), Level::Central);
        assert_eq!(level_of(&s, m), Level::Main);
        assert_eq!(level_of(&s, sub), Level::Sub);
        assert_eq!(level_of(&s, f), Level::Floating);
        let rs = resolve_style(&s, s.root, false);
        assert!(rs.bold);
        assert_eq!(rs.font_size, 30.0);
        let ms = resolve_style(&s, m, false);
        assert_eq!(ms.fill, Some(Color::from_hex("#FF6B6B").unwrap()));
    }

    #[test]
    fn overrides_win_and_line_color_inherits() {
        let mut s = Sheet::new("S", "R");
        let m = s.add_child(s.root, None, "M");
        let sub = s.add_child(m, None, "S");
        let red = Color::from_hex("#FF0000").unwrap();
        s.topic_mut(m).unwrap().style.line_color = Some(red);
        s.topic_mut(sub).unwrap().style.font_size = Some(40.0);
        let rs = resolve_style(&s, sub, true);
        assert_eq!(rs.line_color, red);
        assert_eq!(rs.font_size, 40.0);
    }

    #[test]
    fn rainbow_vs_variant() {
        let t = Theme::builtin("twilight");
        let mut st = ThemeSettings::default();
        assert_ne!(t.branch_color(&st, 0), t.branch_color(&st, 1));
        st.variant = 2;
        assert_eq!(t.branch_color(&st, 0), t.branch_color(&st, 3));
    }

    #[test]
    fn clarity_matches_reference_hierarchy_in_both_appearances() {
        let mut s = Sheet::new("S", "R");
        s.theme.theme_id = "clarity".into();
        let main = s.add_child(s.root, None, "M");
        let sub = s.add_child(main, None, "S");
        let deep = s.add_child(sub, None, "D");
        for dark in [false, true] {
            let root = resolve_style(&s, s.root, dark);
            assert_eq!(root.fill, None);
            assert_eq!(root.shape, TopicShape::None);
            assert!(root.bold);
            assert_eq!(
                root.text_color,
                if dark { Color::WHITE } else { c("#1F2328") }
            );
            let main_style = resolve_style(&s, main, dark);
            assert_eq!(main_style.fill, Some(c("#FF656B")));
            assert_eq!(main_style.text_color, c("#1F2328"));
            assert_eq!(main_style.line_width, 1.5);
            assert_eq!(main_style.line_shape, LineShape::Curve);
            let sub_style = resolve_style(&s, sub, dark);
            assert_eq!(sub_style.shape, TopicShape::RoundedRect);
            assert_eq!(
                sub_style.fill,
                Some(c("#FF656B").lerp(
                    Theme::builtin("clarity").background(&s.theme, dark),
                    if dark { 0.65 } else { 0.8 },
                ))
            );
            assert_eq!(sub_style.border_color, None);
            assert_eq!(sub_style.line_shape, LineShape::RoundedElbow);
            let deep_style = resolve_style(&s, deep, dark);
            assert_eq!(deep_style.fill, None);
            assert_eq!(deep_style.shape, TopicShape::Underline);
            assert_eq!(deep_style.border_color, Some(c("#FF656B")));
        }
    }

    #[test]
    fn clarity_preserves_explicit_styles_and_inherited_connectors() {
        let mut s = Sheet::new("S", "R");
        s.theme.theme_id = "clarity".into();
        let main = s.add_child(s.root, None, "M");
        let sub = s.add_child(main, None, "S");
        s.topic_mut(main).unwrap().style.line_shape = Some(LineShape::Curve);
        let style = &mut s.topic_mut(sub).unwrap().style;
        style.fill = Some(c("#123456"));
        style.text_color = Some(Color::WHITE);
        style.shape = Some(TopicShape::Ellipse);
        style.line_width = Some(4.0);
        let resolved = resolve_style(&s, sub, false);
        assert_eq!(resolved.fill, Some(c("#123456")));
        assert_eq!(resolved.text_color, Color::WHITE);
        assert_eq!(resolved.shape, TopicShape::Ellipse);
        assert_eq!(resolved.line_width, 4.0);
        assert_eq!(resolved.line_shape, LineShape::Curve);
    }
}
