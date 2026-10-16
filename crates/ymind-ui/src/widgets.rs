//! Reusable widgets.

use crate::tokens::{Palette, Tokens};
use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Margin, Response, Role, Sense, Stroke, StrokeKind, Ui, Vec2,
    WidgetInfo, pos2, vec2,
};

/// A square, frameless icon button with a tooltip. `selected` draws an accent background.
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str, selected: bool) -> Response {
    let p = Palette::of_ui(ui);
    let size = Vec2::splat(Tokens::ICON_BUTTON);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, tooltip));
    if ui.is_rect_visible(rect) {
        let bg = if selected {
            p.accent_soft
        } else if resp.hovered() {
            p.surface_alt
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(Tokens::RADIUS_M), bg);
        let color = if !ui.is_enabled() {
            p.text_muted.gamma_multiply(0.5)
        } else if selected {
            p.accent
        } else {
            p.text
        };
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, FontId::proportional(16.0), color);
    }
    resp.on_hover_text(tooltip)
}

/// A toolbar button with an icon above a small caption (like XMind's top toolbar).
pub fn toolbar_button(ui: &mut Ui, icon: &str, label: &str, enabled: bool) -> Response {
    let p = Palette::of_ui(ui);
    let font = FontId::proportional(Tokens::FONT_SMALL);
    let label_w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), p.text).size().x);
    let size = vec2((label_w + 12.0).max(44.0), 40.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));
    if ui.is_rect_visible(rect) {
        if enabled && resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(Tokens::RADIUS_M), p.surface_alt);
        }
        let c = if enabled { p.text } else { p.text_muted.gamma_multiply(0.5) };
        ui.painter().text(pos2(rect.center().x, rect.top() + 13.0), Align2::CENTER_CENTER, icon, FontId::proportional(16.0), c);
        ui.painter().text(pos2(rect.center().x, rect.bottom() - 9.0), Align2::CENTER_CENTER, label, font, if enabled { p.text_muted } else { c });
    }
    resp
}

/// Segmented control (e.g. Format panel tabs "Style | Map"). Returns true when changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let p = Palette::of_ui(ui);
    let mut changed = false;
    let height = 28.0;
    let width = ui.available_width();
    let (outer, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().rect_filled(outer, CornerRadius::same(Tokens::RADIUS_M), p.surface_alt);
    let n = options.len().max(1) as f32;
    let seg_w = (width - 4.0) / n;
    for (i, (opt, label)) in options.iter().enumerate() {
        let r = egui::Rect::from_min_size(pos2(outer.left() + 2.0 + i as f32 * seg_w, outer.top() + 2.0), vec2(seg_w, height - 4.0));
        let resp = ui.interact(r, ui.scope_id().with(("seg", i)), Sense::click());
        let selected = *value == *opt;
        resp.widget_info(|| WidgetInfo::selected(Role::Tab, true, selected, *label));
        if selected {
            ui.painter().rect_filled(r, CornerRadius::same(Tokens::RADIUS_M - 2), p.surface);
            ui.painter().rect_stroke(r, CornerRadius::same(Tokens::RADIUS_M - 2), Stroke::new(1.0, p.border), StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(Tokens::RADIUS_M - 2), p.border.gamma_multiply(0.5));
        }
        ui.painter().text(r.center(), Align2::CENTER_CENTER, *label, FontId::proportional(Tokens::FONT_BODY), if selected { p.text } else { p.text_muted });
        if resp.clicked() && !selected {
            *value = *opt;
            changed = true;
        }
    }
    changed
}

/// iOS-style toggle switch. Returns the response (changed when toggled).
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = Palette::of_ui(ui);
    let size = vec2(32.0, 18.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let value = *on;
    resp.widget_info(|| WidgetInfo::selected(Role::Switch, ui.is_enabled(), value, label));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
        let track = lerp_color(p.border, p.accent, t);
        let r = rect.height() / 2.0;
        ui.painter().rect_filled(rect, CornerRadius::same(r as u8), track);
        let x = egui::lerp((rect.left() + r)..=(rect.right() - r), t);
        ui.painter().circle_filled(pos2(x, rect.center().y), r - 2.0, Color32::WHITE);
    }
    resp
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// A labelled row: label on the left, content right-aligned.
pub fn property_row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of_ui(ui);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(p.text_muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add).inner
    })
    .inner
}

/// A collapsible section with a heading (used in side panels).
pub fn section<R>(ui: &mut Ui, title: &str, default_open: bool, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let p = Palette::of_ui(ui);
    egui::CollapsingHeader::new(egui::RichText::new(title).strong().color(p.text))
        .default_open(default_open)
        .show(ui, add)
        .body_returned
}

/// A row of color swatches. Returns the clicked color.
pub fn color_swatches(ui: &mut Ui, colors: &[Color32], selected: Option<Color32>) -> Option<Color32> {
    let p = Palette::of_ui(ui);
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for &c in colors {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
            let is_sel = selected == Some(c);
            let label = format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
            resp.widget_info(|| WidgetInfo::selected(Role::ColorWell, true, is_sel, &label));
            ui.painter().rect_filled(rect.shrink(2.0), CornerRadius::same(Tokens::RADIUS_S), c);
            let stroke = if is_sel {
                Stroke::new(2.0, p.accent)
            } else if resp.hovered() {
                Stroke::new(1.0, p.text_muted)
            } else {
                Stroke::new(1.0, p.border)
            };
            ui.painter().rect_stroke(rect.shrink(1.0), CornerRadius::same(Tokens::RADIUS_S), stroke, StrokeKind::Inside);
            if resp.on_hover_text(label).clicked() {
                picked = Some(c);
            }
        }
    });
    picked
}

/// A small pill/chip (labels, tags). Returns the response; `removable` adds a ✖ hit area.
pub fn chip(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = FontId::proportional(Tokens::FONT_SMALL);
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font, Color32::WHITE));
    let size = galley.size() + vec2(12.0, 6.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, true, text));
    ui.painter().rect_filled(rect, CornerRadius::same((size.y / 2.0) as u8), color);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
    resp
}

/// A rounded search field with a leading magnifier glyph.
pub fn search_field(ui: &mut Ui, text: &mut String, hint: &str) -> Response {
    let p = Palette::of_ui(ui);
    Frame::new()
        .fill(p.surface_alt)
        .corner_radius(CornerRadius::same(Tokens::RADIUS_M))
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::icons::SEARCH).color(p.text_muted));
                ui.add(
                    egui::TextEdit::singleline(text)
                        .hint_text(hint)
                        .frame(egui::Frame::NONE)
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(ui.scope_id())
            })
            .inner
        })
        .inner
}

/// A raised card frame (floating toolbars, popovers).
pub fn card(ui: &Ui) -> Frame {
    let p = Palette::of_ui(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(Tokens::RADIUS_L))
        .inner_margin(Margin::symmetric(6, 4))
        .shadow(egui::Shadow { offset: [0, 2], blur: 12, spread: 0, color: p.shadow })
}

/// Muted caption text.
pub fn caption(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = Palette::of_ui(ui);
    ui.label(egui::RichText::new(text.into()).size(Tokens::FONT_SMALL).color(p.text_muted))
}

/// A thin vertical divider for toolbars.
pub fn vdivider(ui: &mut Ui) {
    let p = Palette::of_ui(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 24.0), Sense::hover());
    ui.painter().line_segment([rect.center_top(), rect.center_bottom()], Stroke::new(1.0, p.border));
}

/// Primary (accent filled) button.
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    let p = Palette::of_ui(ui);
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(p.accent_text))
            .fill(p.accent)
            .corner_radius(CornerRadius::same(Tokens::RADIUS_M))
            .min_size(vec2(80.0, 28.0)),
    )
}

/// Secondary button.
pub fn secondary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Button::new(text).corner_radius(CornerRadius::same(Tokens::RADIUS_M)).min_size(vec2(80.0, 28.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Tab {
        Style,
        Map,
    }

    #[test]
    fn segmented_switches_tab() {
        let mut tab = Tab::Style;
        let mut harness = Harness::new_ui_state(
            |ui, tab: &mut Tab| {
                segmented(ui, tab, &[(Tab::Style, "Style"), (Tab::Map, "Map")]);
            },
            tab,
        );
        harness.get_by_label("Map").click();
        harness.run();
        tab = *harness.state();
        assert_eq!(tab, Tab::Map);
    }

    #[test]
    fn toggle_flips() {
        let mut harness = Harness::new_ui_state(
            |ui, on: &mut bool| {
                toggle(ui, on, "Rainbow");
            },
            false,
        );
        harness.get_by_label("Rainbow").click();
        harness.run();
        assert!(*harness.state());
    }

    #[test]
    fn swatch_pick() {
        let mut harness = Harness::new_ui_state(
            |ui, c: &mut Option<Color32>| {
                if let Some(x) = color_swatches(ui, &[Color32::RED, Color32::BLUE], *c) {
                    *c = Some(x);
                }
            },
            None,
        );
        harness.get_by_label("#0000FF").click();
        harness.run();
        assert_eq!(*harness.state(), Some(Color32::BLUE));
    }

    #[test]
    fn icon_button_clicks() {
        let mut harness = Harness::new_ui_state(
            |ui, n: &mut u32| {
                if icon_button(ui, crate::icons::UNDO, "Undo", false).clicked() {
                    *n += 1;
                }
            },
            0,
        );
        harness.get_by_label("Undo").click();
        harness.run();
        assert_eq!(*harness.state(), 1);
    }
}
