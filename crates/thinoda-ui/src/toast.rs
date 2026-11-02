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
            let index = self
                .items
                .iter()
                .position(|toast| toast.remaining.is_some())
                .unwrap_or(0);
            self.items.remove(index);
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
        self.show_with_modal(ctx, false);
    }

    /// Keep notifications below modal controls instead of intercepting their clicks.
    pub fn show_with_modal(&mut self, ctx: &Context, modal_open: bool) {
        if self.items.is_empty() {
            return;
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.tick(dt);
        let p = Palette::of(ctx);
        let mut dismiss = None;
        egui::Area::new(Id::unique(("thinoda_toasts", modal_open)))
            .order(if modal_open {
                Order::Middle
            } else {
                Order::Tooltip
            })
            .anchor(Align2::CENTER_BOTTOM, [0.0, -36.0])
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width((ctx.content_rect().width() - 24.0).max(1.0));
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
                                ui.scope(|ui| {
                                    ui.set_max_width((ui.available_width() - 40.0).max(1.0));
                                    ui.add(
                                        egui::Label::new(RichText::new(&t.text).color(p.text))
                                            .wrap(),
                                    );
                                });
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

    #[test]
    fn transient_toasts_do_not_evict_a_sticky_warning() {
        let mut toasts = Toasts::default();
        let warning = toasts.sticky(ToastKind::Warning, "Recovery is unavailable");
        for i in 0..10 {
            toasts.info(i.to_string());
        }
        assert_eq!(toasts.len(), 5);
        assert!(toasts.texts().any(|text| text == "Recovery is unavailable"));
        toasts.dismiss(warning);
        assert!(!toasts.texts().any(|text| text == "Recovery is unavailable"));
    }

    #[test]
    fn modal_and_nonmodal_toasts_can_render_in_one_frame() {
        let context = Context::default();
        let mut toasts = Toasts::default();
        toasts.warning("A warning");
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            toasts.show_with_modal(ui.ctx(), false);
            toasts.show_with_modal(ui.ctx(), true);
        });
        output.textures_delta.clear();
        assert_eq!(toasts.texts().collect::<Vec<_>>(), vec!["A warning"]);
    }

    #[test]
    fn long_toasts_and_dismiss_buttons_fit_narrow_windows() {
        use egui_kittest::{Harness, kittest::Queryable};

        for width in [240.0, 320.0, 390.0, 1252.0] {
            let mut toasts = Toasts::default();
            toasts.sticky(
                ToastKind::Warning,
                "Some recovery data could not be checked. Keep existing storage and save your work. \
                 RecoveryInformationWithoutAnyWhitespaceMustAlsoFitInsideTheWindow.",
            );
            let mut h = Harness::builder()
                .with_size(egui::vec2(width, 600.0))
                .with_context_setup(|ctx| {
                    ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
                    crate::fonts::install_fallbacks(ctx);
                })
                .build_ui_state(|ui, toasts: &mut Toasts| toasts.show(ui.ctx()), toasts);
            h.run_steps(6);
            let rect = h
                .ctx
                .memory(|memory| memory.area_rect(Id::unique(("thinoda_toasts", false))))
                .unwrap();
            assert!(
                h.ctx.content_rect().contains_rect(rect),
                "{width}: {rect:?}"
            );
            let dismiss = h.get_by_label(crate::icons::CLOSE);
            assert!(
                h.ctx.content_rect().contains_rect(dismiss.rect()),
                "{width}: {:?}",
                dismiss.rect()
            );
            dismiss.click();
            h.run_steps(2);
            assert!(h.state().is_empty());
        }
    }
}
