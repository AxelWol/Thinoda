//! Text measurement backed by egui's font system so layout matches rendering exactly.

use egui::{Color32, Context, FontFamily, FontId};
use ymind_core::{TextMeasure, geometry::Vec2};

/// Extra width factor used to simulate bold text (egui's default fonts have no bold face).
pub(crate) const BOLD_WIDTH: f32 = 1.04;

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
        let font = FontId::new(font_size, FontFamily::Proportional);
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
