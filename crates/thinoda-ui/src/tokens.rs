//! Design tokens: semantic colors, radii and spacing.

use egui::{Color32, Context, Theme, Ui};

/// Semantic color palette for one theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Window / panel background.
    pub bg: Color32,
    /// Raised surfaces (cards, toolbars, popups).
    pub surface: Color32,
    /// Alternative surface (hover, inputs, segmented track).
    pub surface_alt: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub accent_soft: Color32,
    pub danger: Color32,
    pub success: Color32,
    pub warning: Color32,
    /// Background of the mind-map canvas.
    pub canvas: Color32,
    pub shadow: Color32,
}

/// Default Thinoda accent (a friendly blue, close to XMind's selection color).
pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(0x2E, 0x7C, 0xF6);

impl Palette {
    pub fn light(accent: Color32) -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(0xF5, 0xF6, 0xF8),
            surface: Color32::WHITE,
            surface_alt: Color32::from_rgb(0xEE, 0xF0, 0xF3),
            border: Color32::from_rgb(0xE1, 0xE4, 0xE8),
            text: Color32::from_rgb(0x1F, 0x23, 0x28),
            text_muted: Color32::from_rgb(0x6B, 0x72, 0x7C),
            accent,
            accent_text: Color32::WHITE,
            accent_soft: accent.gamma_multiply(0.14),
            danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
            success: Color32::from_rgb(0x2D, 0xA4, 0x4E),
            warning: Color32::from_rgb(0xE0, 0x8E, 0x00),
            canvas: Color32::WHITE,
            shadow: Color32::from_black_alpha(40),
        }
    }

    pub fn dark(accent: Color32) -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(0x18, 0x18, 0x1B),
            surface: Color32::from_rgb(0x24, 0x24, 0x28),
            surface_alt: Color32::from_rgb(0x2E, 0x2E, 0x33),
            border: Color32::from_rgb(0x3A, 0x3A, 0x40),
            text: Color32::from_rgb(0xEC, 0xEC, 0xEF),
            text_muted: Color32::from_rgb(0x9A, 0x9A, 0xA3),
            accent,
            accent_text: Color32::WHITE,
            accent_soft: accent.gamma_multiply(0.25),
            danger: Color32::from_rgb(0xFF, 0x6B, 0x6E),
            success: Color32::from_rgb(0x4C, 0xC3, 0x6B),
            warning: Color32::from_rgb(0xFF, 0xB0, 0x2E),
            canvas: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            shadow: Color32::from_black_alpha(120),
        }
    }

    pub fn for_theme(theme: Theme, accent: Color32) -> Self {
        match theme {
            Theme::Dark => Self::dark(accent),
            Theme::Light => Self::light(accent),
        }
    }

    /// Palette of the context's current theme (uses the accent stored by [`crate::install`]).
    pub fn of(ctx: &Context) -> Self {
        let accent = ctx
            .data(|d| d.get_temp::<Color32>(accent_id()))
            .unwrap_or(DEFAULT_ACCENT);
        Self::for_theme(ctx.theme(), accent)
    }

    pub fn of_ui(ui: &Ui) -> Self {
        Self::of(ui.ctx())
    }
}

pub(crate) fn accent_id() -> egui::Id {
    egui::Id::unique("thinoda_ui_accent")
}

/// Sizes used across the design system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens;

impl Tokens {
    pub const RADIUS_S: u8 = 4;
    pub const RADIUS_M: u8 = 8;
    pub const RADIUS_L: u8 = 12;
    pub const SPACE_XS: f32 = 2.0;
    pub const SPACE_S: f32 = 4.0;
    pub const SPACE_M: f32 = 8.0;
    pub const SPACE_L: f32 = 16.0;
    pub const ICON_BUTTON: f32 = 28.0;
    pub const TOOLBAR_HEIGHT: f32 = 44.0;
    pub const FONT_BODY: f32 = 13.0;
    pub const FONT_SMALL: f32 = 11.0;
    pub const FONT_HEADING: f32 = 15.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_have_contrast() {
        for p in [
            Palette::light(DEFAULT_ACCENT),
            Palette::dark(DEFAULT_ACCENT),
        ] {
            let lum = |c: Color32| {
                (c.r() as f32 * 0.299 + c.g() as f32 * 0.587 + c.b() as f32 * 0.114) / 255.0
            };
            assert!(
                (lum(p.text) - lum(p.bg)).abs() > 0.6,
                "text/bg contrast too low"
            );
            assert!((lum(p.text_muted) - lum(p.bg)).abs() > 0.3);
        }
    }
}
