//! Non-blocking toast notifications, stacked at the bottom-center of the screen.

use crate::tokens::{Palette, Tokens};
use egui::{Align2, Context, CornerRadius, Id, Margin, Order, RichText, Stroke};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
struct Toast {
    id: u64,
    kind: ToastKind,
    text: String,
    /// Remaining seconds (None = sticky until dismissed).
    remaining: Option<f32>,
}

/// A queue of toasts. Call [`Toasts::show`] once per frame.
#[derive(Debug, Default)]
pub struct Toasts {
    items: Vec<Toast>,
    next: u64,
}

impl Toasts {
    pub fn add(&mut self, kind: ToastKind, text: impl Into<String>) -> u64 {
        self.push(kind, text.into(), Some(4.0))
    }

    pub fn info(&mut self, text: impl Into<String>) -> u64 {
        self.add(ToastKind::Info, text)
    }

    pub fn success(&mut self, text: impl Into<String>) -> u64 {
        self.add(ToastKind::Success, text)
    }

    pub fn warning(&mut self, text: impl Into<String>) -> u64 {
        self.push(ToastKind::Warning, text.into(), Some(6.0))
    }

    pub fn error(&mut self, text: impl Into<String>) -> u64 {
        self.push(ToastKind::Error, text.into(), Some(8.0))
    }

    pub fn sticky(&mut self, kind: ToastKind, text: impl Into<String>) -> u64 {
        self.push(kind, text.into(), None)
    }

    fn push(&mut self, kind: ToastKind, text: String, remaining: Option<f32>) -> u64 {
        self.next += 1;
        self.items.push(Toast {
            id: self.next,
            kind,
            text,
            remaining,
        });
        if self.items.len() > 5 {
            self.items.remove(0);
        }
        self.next
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|t| t.id != id);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(|t| t.text.as_str())
    }

    /// Advances timers by `dt` seconds (separate from rendering for testability).
    pub fn tick(&mut self, dt: f32) {
        for t in &mut self.items {
            if let Some(r) = &mut t.remaining {
                *r -= dt;
            }
        }
        self.items.retain(|t| t.remaining.is_none_or(|r| r > 0.0));
    }

    pub fn show(&mut self, ctx: &Context) {
        if self.items.is_empty() {
            return;
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.tick(dt);
        let p = Palette::of(ctx);
        let mut dismiss = None;
        egui::Area::new(Id::unique("ymind_toasts"))
            .order(Order::Tooltip)
            .anchor(Align2::CENTER_BOTTOM, [0.0, -36.0])
            .interactable(true)
            .show(ctx, |ui| {
                for t in &self.items {
                    let (icon, color) = match t.kind {
                        ToastKind::Info => (crate::icons::INFO, p.accent),
                        ToastKind::Success => (crate::icons::CHECK, p.success),
                        ToastKind::Warning => (crate::icons::WARNING, p.warning),
                        ToastKind::Error => (crate::icons::ERROR, p.danger),
                    };
                    crate::widgets::card(ui)
                        .inner_margin(Margin::symmetric(12, 8))
                        .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
                        .corner_radius(CornerRadius::same(Tokens::RADIUS_L))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(icon).color(color));
                                ui.label(RichText::new(&t.text).color(p.text));
                                if ui
                                    .small_button(crate::icons::CLOSE)
                                    .on_hover_text("Dismiss")
                                    .clicked()
                                {
                                    dismiss = Some(t.id);
                                }
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        if let Some(id) = dismiss {
            self.dismiss(id);
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toasts_expire() {
        let mut t = Toasts::default();
        t.info("a");
        t.error("b");
        let s = t.sticky(ToastKind::Warning, "c");
        t.tick(5.0);
        assert_eq!(t.texts().collect::<Vec<_>>(), vec!["b", "c"]);
        t.tick(10.0);
        assert_eq!(t.len(), 1);
        t.dismiss(s);
        assert!(t.is_empty());
    }

    #[test]
    fn toasts_capped() {
        let mut t = Toasts::default();
        for i in 0..10 {
            t.info(i.to_string());
        }
        assert_eq!(t.len(), 5);
        assert_eq!(t.texts().next(), Some("5"));
    }
}
