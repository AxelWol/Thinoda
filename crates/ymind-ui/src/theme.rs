//! Installs the yMind visual style into an egui context.

use crate::tokens::{Palette, Tokens, accent_id};
use egui::{Color32, Context, CornerRadius, FontId, Margin, Shadow, Stroke, TextStyle, Theme, ThemePreference, Visuals};
use serde::{Deserialize, Serialize};

/// User preference for light/dark appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Follow the operating system.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn preference(self) -> ThemePreference {
        match self {
            ThemeMode::System => ThemePreference::System,
            ThemeMode::Light => ThemePreference::Light,
            ThemeMode::Dark => ThemePreference::Dark,
        }
    }
}

/// Applies the design system (both light and dark variants) and the theme preference.
pub fn install(ctx: &Context, mode: ThemeMode, accent: Color32) {
    ctx.data_mut(|d| d.insert_temp(accent_id(), accent));
    for theme in [Theme::Light, Theme::Dark] {
        let p = Palette::for_theme(theme, accent);
        ctx.style_mut_of(theme, |style| {
            style.visuals = visuals(&p);
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(8.0, 4.0);
            style.spacing.interact_size.y = 26.0;
            style.spacing.menu_margin = Margin::same(6);
            style.spacing.window_margin = Margin::same(12);
            style.text_styles.insert(TextStyle::Body, FontId::proportional(Tokens::FONT_BODY));
            style.text_styles.insert(TextStyle::Button, FontId::proportional(Tokens::FONT_BODY));
            style.text_styles.insert(TextStyle::Small, FontId::proportional(Tokens::FONT_SMALL));
            style.text_styles.insert(TextStyle::Heading, FontId::proportional(Tokens::FONT_HEADING + 3.0));
        });
    }
    ctx.set_theme(mode.preference());
}

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    let r = CornerRadius::same(Tokens::RADIUS_M);
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(Tokens::RADIUS_L);
    v.menu_corner_radius = CornerRadius::same(Tokens::RADIUS_L);
    v.window_shadow = Shadow { offset: [0, 6], blur: 24, spread: 0, color: p.shadow };
    v.popup_shadow = Shadow { offset: [0, 4], blur: 16, spread: 0, color: p.shadow };
    v.extreme_bg_color = if p.dark { Color32::from_rgb(0x1B, 0x1B, 0x1F) } else { Color32::WHITE };
    v.faint_bg_color = p.surface_alt;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.override_text_color = None;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = p.surface_alt;
    w.inactive.weak_bg_fill = p.surface_alt;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = p.border;
    w.hovered.weak_bg_fill = p.border;
    w.hovered.bg_stroke = Stroke::NONE;
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.accent_soft;
    w.active.weak_bg_fill = p.accent_soft;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.surface_alt;
    w.open.weak_bg_fill = p.surface_alt;
    w.open.corner_radius = r;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_sets_both_themes() {
        let ctx = Context::default();
        install(&ctx, ThemeMode::Dark, Color32::RED);
        assert_eq!(ctx.theme(), Theme::Dark);
        assert_eq!(Palette::of(&ctx).accent, Color32::RED);
        install(&ctx, ThemeMode::Light, Color32::RED);
        assert_eq!(ctx.theme(), Theme::Light);
        assert!(!ctx.global_style().visuals.dark_mode);
    }
}
