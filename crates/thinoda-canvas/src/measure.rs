//! Text measurement backed by egui's font system so layout matches rendering exactly.

use egui::{Color32, Context, FontFamily, FontId};
use thinoda_core::{TextMeasure, geometry::Vec2};

/// Extra width factor used to simulate bold text (egui's default fonts have no bold face).
pub(crate) const BOLD_WIDTH: f32 = 1.04;

pub(crate) fn font_family(ctx: &Context, name: Option<&str>) -> FontFamily {
    match name {
        Some("monospace") => FontFamily::Monospace,
        None | Some("" | "sans-serif") => FontFamily::Proportional,
        Some(name) => {
            let family = FontFamily::Name(name.into());
            if ctx.fonts(|fonts| fonts.definitions().families.contains_key(&family)) {
                family
            } else {
                FontFamily::Proportional
            }
        }
    }
}

/// A [`TextMeasure`] that lays text out with egui.
#[derive(Clone)]
pub struct EguiMeasure {
    ctx: Context,
}

impl EguiMeasure {
    pub fn new(ctx: &Context) -> Self {
        Self { ctx: ctx.clone() }
    }
}

impl TextMeasure for EguiMeasure {
    fn measure(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> Vec2 {
        self.measure_with_family(text, font_size, bold, max_width, None)
    }

    fn measure_with_family(
        &self,
        text: &str,
        font_size: f32,
        bold: bool,
        max_width: f32,
        family: Option<&str>,
    ) -> Vec2 {
        let font = FontId::new(font_size, font_family(&self.ctx, family));
        let g = self.ctx.fonts_mut(|f| {
            f.layout(
                text.to_owned(),
                font,
                Color32::BLACK,
                max_width / if bold { BOLD_WIDTH } else { 1.0 },
            )
        });
        let s = g.size();
        Vec2::new(
            (s.x * if bold { BOLD_WIDTH } else { 1.0 }).ceil(),
            s.y.ceil(),
        )
    }
}
