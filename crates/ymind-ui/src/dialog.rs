//! Modal dialogs.

use crate::tokens::Palette;
use egui::{Context, Id, Modal, RichText};

/// A button shown in a dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogButton {
    Primary,
    Secondary,
    Danger,
}

/// Shows a modal dialog with a title, message and buttons.
/// Returns `Some(index)` of the pressed button, or `Some(usize::MAX)` when dismissed (Esc / backdrop).
pub fn confirm_dialog(
    ctx: &Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    title: &str,
    message: &str,
    buttons: &[(&str, DialogButton)],
) -> Option<usize> {
    let p = Palette::of(ctx);
    let mut pressed = None;
    let resp = Modal::new(Id::unique(id)).show(ctx, |ui| {
        ui.set_max_width(440.0);
        ui.label(RichText::new(title).heading().color(p.text));
        ui.add_space(6.0);
        ui.label(RichText::new(message).color(p.text_muted));
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (i, (label, kind)) in buttons.iter().enumerate().rev() {
                let r = match kind {
                    DialogButton::Primary => crate::widgets::primary_button(ui, label),
                    DialogButton::Secondary => crate::widgets::secondary_button(ui, label),
                    DialogButton::Danger => ui.add(
                        egui::Button::new(RichText::new(*label).color(egui::Color32::WHITE))
                            .fill(p.danger)
                            .min_size(egui::vec2(80.0, 28.0)),
                    ),
                };
                if r.clicked() {
                    pressed = Some(i);
                }
            }
        });
    });
    if pressed.is_none() && resp.should_close() {
        return Some(usize::MAX);
    }
    pressed
}
