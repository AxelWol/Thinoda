//! Reusable widgets.

use crate::tokens::{Palette, Tokens};
use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Margin, Response, Role, Sense, Stroke,
    StrokeKind, Ui, Vec2, WidgetInfo, pos2, vec2,
};

/// Reject click activation through canceled/multitouch sequences and their pointer release.
/// Call each frame, including when the guarded controls are hidden.
pub fn touch_activation_allowed(ctx: &egui::Context) -> bool {
    let (multi_touch, touching, primary_down, canceled) = ctx.input(|input| {
        (
            input.multi_touch().is_some(),
            input.any_touches(),
            input.pointer.primary_down(),
            input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Touch {
                        phase: egui::TouchPhase::Cancel,
                        ..
                    }
                )
            }),
        )
    });
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|data| {
        let id = egui::Id::unique("touch_activation_guard");
        let previous = data.get_temp::<(u64, bool, bool)>(id);
        if let Some((previous_frame, _, allowed)) = previous
            && previous_frame == frame
        {
            return allowed;
        }
        let suppressed =
            previous.is_some_and(|(_, suppressed, _)| suppressed) || multi_touch || canceled;
        // Backends can deliver the synthesized pointer release after the final touch.
        data.insert_temp(
            id,
            (frame, suppressed && (touching || primary_down), !suppressed),
        );
        !suppressed
    })
}

/// Selectable preview with a wrapped caption, accessible name and keyboard activation.
pub fn preview_card(
    ui: &mut Ui,
    label: &str,
    selected: bool,
    width: f32,
    preview_height: f32,
    draw: impl FnOnce(&Ui, egui::Rect),
) -> (Response, bool) {
    let touch_allowed = touch_activation_allowed(ui.ctx());
    let galley = ui.painter().layout(
        label.to_string(),
        FontId::proportional(11.0),
        Color32::PLACEHOLDER,
        (width - 10.0).max(1.0),
    );
    let (rect, response) = ui.allocate_exact_size(
        vec2(
            width,
            (preview_height + 8.0 + galley.size().y).max(preview_height + 36.0),
        ),
        Sense::click(),
    );
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, label));
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, selected);
        ui.painter().rect(
            rect,
            6,
            visuals.bg_fill,
            visuals.bg_stroke,
            StrokeKind::Inside,
        );
        if response.has_focus() || selected {
            ui.painter()
                .rect_stroke(rect, 6, ui.visuals().selection.stroke, StrokeKind::Inside);
        }
        draw(
            ui,
            egui::Rect::from_min_size(rect.min, vec2(width, preview_height)).shrink(2.0),
        );
        ui.painter().galley(
            pos2(
                rect.center().x - galley.size().x / 2.0,
                rect.top() + preview_height + 4.0,
            ),
            galley,
            visuals.text_color(),
        );
    }
    let keyboard = ui.is_enabled()
        && response.has_focus()
        && ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || input.consume_key(egui::Modifiers::NONE, egui::Key::Space)
        });
    let activated = ui.is_enabled() && ((touch_allowed && response.clicked()) || keyboard);
    (response.on_hover_text(label), activated)
}

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
        ui.painter()
            .rect_filled(rect, CornerRadius::same(Tokens::RADIUS_M), bg);
        if resp.has_focus() && ui.is_enabled() {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(Tokens::RADIUS_M),
                Stroke::new(2.0, p.text),
                StrokeKind::Inside,
            );
        }
        let color = if !ui.is_enabled() {
            p.text_muted.gamma_multiply(0.5)
        } else if selected {
            p.accent
        } else {
            p.text
        };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(16.0),
            color,
        );
    }
    resp.on_hover_text(tooltip)
}

/// A toolbar button with an icon above a small caption (like XMind's top toolbar).
pub fn toolbar_button(ui: &mut Ui, icon: &str, label: &str, enabled: bool) -> Response {
    let enabled = enabled && ui.is_enabled();
    let p = Palette::of_ui(ui);
    let font = FontId::proportional(Tokens::FONT_SMALL);
    let label_w = ui.fonts_mut(|f| {
        f.layout_no_wrap(label.to_owned(), font.clone(), p.text)
            .size()
            .x
    });
    let size = vec2((label_w + 12.0).max(44.0), 40.0);
    let (rect, resp) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));
    if ui.is_rect_visible(rect) {
        if enabled && resp.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(Tokens::RADIUS_M), p.surface_alt);
        }
        if enabled && resp.has_focus() {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(Tokens::RADIUS_M),
                Stroke::new(2.0, p.text),
                StrokeKind::Inside,
            );
        }
        let c = if enabled {
            p.text
        } else {
            p.text_muted.gamma_multiply(0.5)
        };
        ui.painter().text(
            pos2(rect.center().x, rect.top() + 13.0),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(16.0),
            c,
        );
        ui.painter().text(
            pos2(rect.center().x, rect.bottom() - 9.0),
            Align2::CENTER_CENTER,
            label,
            font,
            if enabled { p.text_muted } else { c },
        );
    }
    resp
}

/// Segmented control (e.g. Format panel tabs "Style | Map"). Returns true when changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let touch_allowed = touch_activation_allowed(ui.ctx());
    let p = Palette::of_ui(ui);
    let mut changed = false;
    let height = 28.0;
    let width = ui.available_width();
    let (outer, outer_resp) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter()
        .rect_filled(outer, CornerRadius::same(Tokens::RADIUS_M), p.surface_alt);
    let n = options.len().max(1) as f32;
    let seg_w = (width - 4.0) / n;
    for (i, (opt, label)) in options.iter().enumerate() {
        let r = egui::Rect::from_min_size(
            pos2(outer.left() + 2.0 + i as f32 * seg_w, outer.top() + 2.0),
            vec2(seg_w, height - 4.0),
        );
        let resp = ui.interact(r, outer_resp.id.with(("seg", i)), Sense::click());
        let selected = *value == *opt;
        resp.widget_info(|| WidgetInfo::selected(Role::Tab, ui.is_enabled(), selected, *label));
        if selected {
            ui.painter()
                .rect_filled(r, CornerRadius::same(Tokens::RADIUS_M - 2), p.surface);
            ui.painter().rect_stroke(
                r,
                CornerRadius::same(Tokens::RADIUS_M - 2),
                Stroke::new(1.0, p.border),
                StrokeKind::Inside,
            );
        } else if resp.hovered() {
            ui.painter().rect_filled(
                r,
                CornerRadius::same(Tokens::RADIUS_M - 2),
                p.border.gamma_multiply(0.5),
            );
        }
        if resp.has_focus() && ui.is_enabled() {
            ui.painter().rect_stroke(
                r,
                CornerRadius::same(Tokens::RADIUS_M - 2),
                Stroke::new(2.0, p.text),
                StrokeKind::Inside,
            );
        }
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            *label,
            FontId::proportional(Tokens::FONT_BODY),
            if selected { p.text } else { p.text_muted },
        );
        if touch_allowed && resp.clicked() && !selected {
            *value = *opt;
            changed = true;
        }
    }
    changed
}

/// iOS-style toggle switch. Returns the response (changed when toggled).
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let touch_allowed = touch_activation_allowed(ui.ctx());
    let p = Palette::of_ui(ui);
    let size = vec2(32.0, 18.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if touch_allowed && resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let value = *on;
    resp.widget_info(|| WidgetInfo::selected(Role::Switch, ui.is_enabled(), value, label));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
        let track = lerp_color(p.border, p.accent, t);
        let r = rect.height() / 2.0;
        ui.painter()
            .rect_filled(rect, CornerRadius::same(r as u8), track);
        let x = egui::lerp((rect.left() + r)..=(rect.right() - r), t);
        ui.painter()
            .circle_filled(pos2(x, rect.center().y), r - 2.0, Color32::WHITE);
        if resp.has_focus() && ui.is_enabled() {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(r as u8),
                Stroke::new(2.0, p.text),
                StrokeKind::Inside,
            );
        }
    }
    resp
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

/// A labelled row: label on the left, content right-aligned.
pub fn property_row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of_ui(ui);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(p.text_muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add)
            .inner
    })
    .inner
}

/// Like [`property_row`] for a single input control: the label becomes the control's
/// accessible name (screen readers, UI tests).
pub fn property_field(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> Response) -> Response {
    let p = Palette::of_ui(ui);
    ui.horizontal(|ui| {
        let l = ui.label(egui::RichText::new(label).color(p.text_muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add)
            .inner
            .labelled_by(l.id)
    })
    .inner
}

/// A collapsible section with a heading (used in side panels).
pub fn section<R>(
    ui: &mut Ui,
    title: &str,
    default_open: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let p = Palette::of_ui(ui);
    egui::CollapsingHeader::new(egui::RichText::new(title).strong().color(p.text))
        .default_open(default_open)
        .show(ui, add)
        .body_returned
}

/// A row of color swatches, including the current color if it is not a preset.
/// Returns the clicked color.
pub fn color_swatches(
    ui: &mut Ui,
    colors: &[Color32],
    selected: Option<Color32>,
) -> Option<Color32> {
    let p = Palette::of_ui(ui);
    let mut picked = None;
    ui.vertical(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            let custom = selected.filter(|c| !colors.contains(c));
            for c in custom.into_iter().chain(colors.iter().copied()) {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                let resp = ui.interact(
                    rect,
                    ui.unique_id().with(("color-swatch", c)),
                    Sense::click(),
                );
                let is_sel = selected == Some(c);
                let label = format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
                resp.widget_info(|| {
                    WidgetInfo::selected(Role::ColorWell, ui.is_enabled(), is_sel, &label)
                });
                ui.painter()
                    .rect_filled(rect.shrink(2.0), CornerRadius::same(Tokens::RADIUS_S), c);
                let stroke = if is_sel {
                    Stroke::new(2.0, p.accent)
                } else if resp.hovered() {
                    Stroke::new(1.0, p.text_muted)
                } else {
                    Stroke::new(1.0, p.border)
                };
                ui.painter().rect_stroke(
                    rect.shrink(1.0),
                    CornerRadius::same(Tokens::RADIUS_S),
                    stroke,
                    StrokeKind::Inside,
                );
                if resp.has_focus() && ui.is_enabled() {
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(Tokens::RADIUS_S),
                        Stroke::new(2.0, p.text),
                        StrokeKind::Inside,
                    );
                }
                if is_sel {
                    let contrast = if c.intensity() > 0.5 {
                        Color32::BLACK
                    } else {
                        Color32::WHITE
                    };
                    ui.painter().line_segment(
                        [rect.min + vec2(5.0, 10.0), rect.min + vec2(9.0, 14.0)],
                        Stroke::new(2.0, contrast),
                    );
                    ui.painter().line_segment(
                        [rect.min + vec2(9.0, 14.0), rect.min + vec2(15.0, 6.0)],
                        Stroke::new(2.0, contrast),
                    );
                }
                if touch_activation_allowed(ui.ctx()) && resp.on_hover_text(label).clicked() {
                    picked = Some(c);
                }
            }
        });
        if let Some(c) = selected {
            let hex = if c.a() == 255 {
                format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
            } else {
                format!("#{:02X}{:02X}{:02X}{:02X}", c.r(), c.g(), c.b(), c.a())
            };
            ui.label(egui::RichText::new(hex).small().color(p.text_muted));
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
    ui.painter()
        .rect_filled(rect, CornerRadius::same((size.y / 2.0) as u8), color);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
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
                .accessible_name(hint)
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
        .shadow(egui::Shadow {
            offset: [0, 2],
            blur: 12,
            spread: 0,
            color: p.shadow,
        })
}

/// Muted caption text.
pub fn caption(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = Palette::of_ui(ui);
    ui.label(
        egui::RichText::new(text.into())
            .size(Tokens::FONT_SMALL)
            .color(p.text_muted),
    )
}

/// A thin vertical divider for toolbars.
pub fn vdivider(ui: &mut Ui) {
    let p = Palette::of_ui(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 24.0), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, p.border),
    );
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
    ui.add(
        egui::Button::new(text)
            .corner_radius(CornerRadius::same(Tokens::RADIUS_M))
            .min_size(vec2(80.0, 28.0)),
    )
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
    fn preview_card_exposes_selection_and_consumes_keyboard_activation() {
        use egui_kittest::kittest::NodeT;
        for key in [egui::Key::Enter, egui::Key::Space] {
            let mut h = Harness::new_ui_state(
                move |ui, state: &mut (bool, bool, bool)| {
                    ui.add_enabled_ui(state.0, |ui| {
                        let (_, activated) = preview_card(
                            ui,
                            "Example preview",
                            state.1,
                            120.0,
                            72.0,
                            |ui, rect| {
                                ui.painter().rect_filled(rect, 4, Color32::BLUE);
                            },
                        );
                        if activated {
                            state.1 = true;
                        }
                    });
                    if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key)) {
                        state.2 = true;
                    }
                },
                (true, false, false),
            );
            h.get_by_role_and_label(Role::Button, "Example preview")
                .focus();
            h.run_steps(2);
            h.key_press(key);
            h.run_steps(3);
            assert!(h.state().1);
            assert!(!h.state().2);
            assert_eq!(
                h.get_by_role_and_label(Role::Button, "Example preview")
                    .accesskit_node()
                    .toggled(),
                Some(egui::accesskit::Toggled::True)
            );
            h.state_mut().0 = false;
            h.state_mut().1 = false;
            h.run_steps(2);
            h.key_press(key);
            h.run_steps(2);
            assert!(!h.state().1);
        }
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
    fn settings_controls_expose_focus_and_disabled_state() {
        use egui_kittest::kittest::NodeT;
        for switch in [false, true] {
            for dark in [false, true] {
                for key in [egui::Key::Enter, egui::Key::Space] {
                    let mut h = Harness::new_ui_state(
                        move |ui, state: &mut (bool, bool, Tab, usize)| {
                            ui.ctx().set_visuals(if dark {
                                egui::Visuals::dark()
                            } else {
                                egui::Visuals::light()
                            });
                            ui.add_enabled_ui(state.0, |ui| {
                                let changed = if switch {
                                    toggle(ui, &mut state.1, "Rainbow").changed()
                                } else {
                                    segmented(
                                        ui,
                                        &mut state.2,
                                        &[(Tab::Style, "Style"), (Tab::Map, "Map")],
                                    )
                                };
                                state.3 += usize::from(changed);
                            });
                        },
                        (true, false, Tab::Style, 0),
                    );
                    h.run_steps(2);
                    let (role, label) = if switch {
                        (Role::Switch, "Rainbow")
                    } else {
                        (Role::Tab, "Map")
                    };
                    h.get_by_role_and_label(role, label).focus();
                    h.run_steps(2);
                    let node = h.get_by_role_and_label(role, label);
                    assert!(node.is_focused());
                    let rect = node.rect();
                    assert!(
                        h.output().shapes.iter().any(|shape| {
                            matches!(&shape.shape, egui::Shape::Rect(shape)
                            if shape.rect == rect && shape.stroke.width >= 2.0
                                && shape.stroke.color != Color32::TRANSPARENT)
                        }),
                        "Settings controls need a visible focus indicator"
                    );
                    h.key_press(key);
                    h.run_steps(3);
                    assert_eq!(h.state().3, 1);
                    if switch {
                        assert!(h.state().1);
                    } else {
                        assert_eq!(h.state().2, Tab::Map);
                    }
                    h.state_mut().0 = false;
                    h.run_steps(2);
                    assert!(
                        h.get_by_role_and_label(role, label)
                            .accesskit_node()
                            .is_disabled()
                    );
                    h.key_press(key);
                    h.run_steps(2);
                    assert_eq!(h.state().3, 1);
                }
            }
        }
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
    fn adjacent_swatch_rows_edit_only_their_own_color() {
        for dark in [false, true] {
            let mut h = Harness::new_ui_state(
                move |ui, colors: &mut [Option<Color32>; 2]| {
                    ui.ctx().set_visuals(if dark {
                        egui::Visuals::dark()
                    } else {
                        egui::Visuals::light()
                    });
                    for (label, color) in ["Fill", "Border"].into_iter().zip(colors) {
                        property_row(ui, label, |ui| {
                            if let Some(picked) =
                                color_swatches(ui, &[Color32::RED, Color32::BLUE], *color)
                            {
                                *color = Some(picked);
                            }
                        });
                    }
                },
                [Some(Color32::RED), Some(Color32::RED)],
            );
            h.run_steps(3);
            assert_eq!(
                h.query_all_by_role_and_label(Role::ColorWell, "#0000FF")
                    .count(),
                2,
                "Fill and Border must expose separate swatches"
            );
            h.query_all_by_role_and_label(Role::ColorWell, "#0000FF")
                .next()
                .unwrap()
                .click();
            h.run_steps(3);
            assert_eq!(*h.state(), [Some(Color32::BLUE), Some(Color32::RED)]);
            h.query_all_by_role_and_label(Role::ColorWell, "#0000FF")
                .nth(1)
                .unwrap()
                .click();
            h.run_steps(3);
            assert_eq!(*h.state(), [Some(Color32::BLUE), Some(Color32::BLUE)]);
        }
    }

    #[test]
    fn swatches_keep_visible_keyboard_focus_when_custom_color_disappears() {
        use egui_kittest::kittest::NodeT;
        for dark in [false, true] {
            for key in [egui::Key::Enter, egui::Key::Space] {
                let custom = Color32::from_rgb(0x12, 0x34, 0x56);
                let mut h = Harness::new_ui_state(
                    move |ui, state: &mut (bool, Option<Color32>, usize)| {
                        ui.ctx().set_visuals(if dark {
                            egui::Visuals::dark()
                        } else {
                            egui::Visuals::light()
                        });
                        ui.add_enabled_ui(state.0, |ui| {
                            if let Some(color) =
                                color_swatches(ui, &[Color32::RED, Color32::BLUE], state.1)
                            {
                                state.1 = Some(color);
                                state.2 += 1;
                            }
                        });
                    },
                    (true, Some(custom), 0),
                );
                h.run_steps(2);
                for label in ["#123456", "#FF0000", "#0000FF"] {
                    h.key_press(egui::Key::Tab);
                    h.run_steps(2);
                    let node = h.get_by_role_and_label(Role::ColorWell, label);
                    assert!(node.is_focused(), "Tab should reach {label}");
                    let rect = node.rect();
                    assert!(
                        h.output().shapes.iter().any(|shape| {
                            matches!(&shape.shape, egui::Shape::Rect(shape)
                            if shape.rect == rect && shape.stroke.width >= 2.0
                                && shape.stroke.color != Color32::TRANSPARENT)
                        }),
                        "Color swatches need a distinct keyboard-focus outline"
                    );
                }
                h.key_press(key);
                h.run_steps(3);
                assert_eq!(h.state().1, Some(Color32::BLUE));
                assert_eq!(h.state().2, 1);
                assert!(
                    h.query_by_role_and_label(Role::ColorWell, "#123456")
                        .is_none()
                );
                assert!(
                    h.get_by_role_and_label(Role::ColorWell, "#0000FF")
                        .is_focused(),
                    "Removing the custom swatch must not move focus off the chosen preset"
                );
                h.state_mut().0 = false;
                h.run_steps(2);
                assert!(
                    h.get_by_role_and_label(Role::ColorWell, "#0000FF")
                        .accesskit_node()
                        .is_disabled()
                );
                h.key_press(key);
                h.run_steps(2);
                assert_eq!(h.state().2, 1);
            }
        }
    }

    #[test]
    fn swatches_show_and_refresh_colors_outside_the_palette() {
        use egui_kittest::kittest::NodeT;
        let mut h = Harness::new_ui_state(
            |ui, c: &mut Option<Color32>| {
                color_swatches(ui, &[Color32::RED, Color32::BLUE], *c);
            },
            Some(Color32::from_rgb(0xFF, 0xA3, 0x66)),
        );
        assert_eq!(
            h.get_by_role_and_label(Role::ColorWell, "#FFA366")
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        h.get_by_role_and_label(Role::Label, "#FFA366");
        *h.state_mut() = Some(Color32::from_rgb(0x12, 0x34, 0x56));
        h.run_steps(3);
        assert!(
            h.query_by_role_and_label(Role::ColorWell, "#FFA366")
                .is_none()
        );
        assert_eq!(
            h.get_by_role_and_label(Role::ColorWell, "#123456")
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        *h.state_mut() = Some(Color32::BLUE);
        h.run_steps(3);
        assert_eq!(
            h.query_all_by_role_and_label(Role::ColorWell, "#0000FF")
                .count(),
            1
        );
        *h.state_mut() = None;
        h.run_steps(3);
        assert!(
            h.query_by_role_and_label(Role::ColorWell, "#123456")
                .is_none()
        );
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

    #[test]
    fn toolbar_controls_show_focus_and_activate_once_from_the_keyboard() {
        use egui_kittest::kittest::NodeT;
        for captioned in [false, true] {
            for dark in [false, true] {
                for selected in [false, true] {
                    for key in [egui::Key::Enter, egui::Key::Space] {
                        let mut h = Harness::new_ui_state(
                            move |ui, state: &mut (bool, u32)| {
                                ui.ctx().set_visuals(if dark {
                                    egui::Visuals::dark()
                                } else {
                                    egui::Visuals::light()
                                });
                                ui.add_enabled_ui(state.0, |ui| {
                                    let response = if captioned {
                                        toolbar_button(ui, crate::icons::UNDO, "Undo", true)
                                    } else {
                                        icon_button(ui, crate::icons::UNDO, "Undo", selected)
                                    };
                                    if response.clicked() {
                                        state.1 += 1;
                                    }
                                });
                            },
                            (true, 0),
                        );
                        h.run_steps(2);
                        h.key_press(egui::Key::Tab);
                        h.run_steps(2);
                        let node = h.get_by_role_and_label(Role::Button, "Undo");
                        assert!(node.is_focused());
                        let rect = node.rect();
                        assert!(
                            h.output().shapes.iter().any(|shape| {
                                matches!(&shape.shape, egui::Shape::Rect(shape)
                                if shape.rect == rect && shape.stroke.width >= 2.0
                                    && shape.stroke.color != Color32::TRANSPARENT)
                            }),
                            "Focused toolbar control needs a visible outline"
                        );
                        h.key_press(key);
                        h.run_steps(3);
                        assert_eq!(h.state().1, 1);
                        h.state_mut().0 = false;
                        h.run_steps(2);
                        assert!(h.get_by_label("Undo").accesskit_node().is_disabled());
                        h.key_press(key);
                        h.run_steps(3);
                        assert_eq!(h.state().1, 1, "Disabled control must not activate");
                    }
                }
            }
        }
    }

    #[test]
    fn search_field_has_a_resolvable_accessible_name() {
        let mut harness = Harness::new_ui_state(
            |ui, query: &mut String| {
                search_field(ui, query, "Search topics");
            },
            String::new(),
        );
        harness.get_by_label("Search topics").click();
        harness.run();
        harness.get_by_label("Search topics").type_text("roadmap");
        harness.run();
        assert_eq!(harness.state(), "roadmap");
    }
}
