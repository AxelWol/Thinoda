//! Original, font-independent line icons and adaptive ribbon command controls.

use egui::{Align2, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::{Palette, widgets};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    File,
    Folder,
    Save,
    Clipboard,
    Copy,
    Cut,
    Undo,
    Redo,
    Topic,
    Child,
    Edit,
    Up,
    Down,
    Indent,
    Outdent,
    Delete,
    Link,
    Format,
    Layout,
    Search,
    Print,
    Export,
    Settings,
    Help,
    Spark,
    Sheet,
    Zoom,
    Play,
    Chevron,
    Camera,
    Close,
    Power,
    Home,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Large,
    Labeled,
    Icon,
}

impl Glyph {
    pub fn paint(self, ui: &Ui, rect: Rect, color: egui::Color32) {
        let painter = ui.painter();
        let point = |x: f32, y: f32| {
            pos2(
                rect.left() + rect.width() * x / 24.0,
                rect.top() + rect.height() * y / 24.0,
            )
        };
        let stroke = Stroke::new((rect.width() / 16.0).clamp(1.2, 1.8), color);
        let line = |points: &[(f32, f32)]| {
            painter.add(egui::Shape::line(
                points.iter().map(|&(x, y)| point(x, y)).collect(),
                stroke,
            ));
        };
        let box_at = |x: f32, y: f32, w: f32, h: f32| {
            painter.rect_stroke(
                Rect::from_min_max(point(x, y), point(x + w, y + h)),
                2,
                stroke,
                StrokeKind::Inside,
            );
        };
        match self {
            Self::File | Self::Sheet => {
                line(&[
                    (5., 2.),
                    (15., 2.),
                    (20., 7.),
                    (20., 22.),
                    (5., 22.),
                    (5., 2.),
                ]);
                line(&[(15., 2.), (15., 8.), (20., 8.)]);
                line(&[(8., 13.), (16., 13.)]);
                line(&[(8., 17.), (14., 17.)]);
            }
            Self::Folder => {
                line(&[
                    (3., 20.),
                    (3., 5.),
                    (10., 5.),
                    (12., 8.),
                    (21., 8.),
                    (21., 20.),
                    (3., 20.),
                ]);
                line(&[(3., 11.), (21., 11.)]);
            }
            Self::Save => {
                box_at(3., 3., 18., 18.);
                box_at(7., 3., 9., 6.);
                box_at(7., 14., 10., 7.);
            }
            Self::Copy => {
                box_at(3., 3., 13., 15.);
                box_at(8., 7., 13., 15.);
            }
            Self::Clipboard => {
                box_at(5., 4., 15., 18.);
                box_at(8., 2., 9., 5.);
                line(&[(9., 12.), (16., 12.)]);
                line(&[(9., 16.), (15., 16.)]);
            }
            Self::Cut => {
                painter.circle_stroke(point(6., 18.), rect.width() * 0.13, stroke);
                painter.circle_stroke(point(18., 18.), rect.width() * 0.13, stroke);
                line(&[(8., 15.), (19., 2.)]);
                line(&[(16., 15.), (5., 2.)]);
            }
            Self::Undo | Self::Redo => {
                let mirror = |x| if self == Self::Redo { 24. - x } else { x };
                let points: Vec<_> = [
                    (8., 4.),
                    (3., 9.),
                    (8., 14.),
                    (3., 9.),
                    (14., 9.),
                    (20., 12.),
                    (20., 18.),
                ]
                .into_iter()
                .map(|(x, y)| (mirror(x), y))
                .collect();
                line(&points);
            }
            Self::Topic | Self::Child | Self::Layout => {
                box_at(2., 8., 8., 8.);
                box_at(16., 2., 7., 6.);
                box_at(16., 16., 7., 6.);
                line(&[(10., 12.), (13., 12.), (13., 5.), (16., 5.)]);
                line(&[(13., 12.), (13., 19.), (16., 19.)]);
                if self == Self::Child {
                    line(&[(17., 11.), (23., 11.)]);
                    line(&[(20., 8.), (20., 14.)]);
                }
            }
            Self::Edit => {
                line(&[
                    (4., 20.),
                    (6., 14.),
                    (17., 3.),
                    (21., 7.),
                    (10., 18.),
                    (4., 20.),
                ]);
                line(&[(14., 6.), (18., 10.)]);
            }
            Self::Up | Self::Down => {
                let y = |y| if self == Self::Down { 24. - y } else { y };
                line(&[(5., y(10.)), (12., y(3.)), (19., y(10.))]);
                line(&[(12., y(3.)), (12., y(21.))]);
            }
            Self::Indent | Self::Outdent => {
                line(&[(3., 4.), (21., 4.)]);
                line(&[(11., 9.), (21., 9.)]);
                line(&[(11., 14.), (21., 14.)]);
                line(&[(3., 20.), (21., 20.)]);
                if self == Self::Indent {
                    line(&[(3., 8.), (7., 12.), (3., 16.)]);
                } else {
                    line(&[(7., 8.), (3., 12.), (7., 16.)]);
                }
            }
            Self::Delete => {
                line(&[(3., 6.), (21., 6.)]);
                box_at(6., 6., 12., 15.);
                line(&[(9., 3.), (15., 3.)]);
                line(&[(10., 10.), (10., 17.)]);
                line(&[(14., 10.), (14., 17.)]);
            }
            Self::Link => {
                box_at(2., 5., 13., 8.);
                box_at(9., 11., 13., 8.);
                line(&[(8., 9.), (16., 15.)]);
            }
            Self::Format => {
                line(&[(4., 20.), (12., 3.), (20., 20.)]);
                line(&[(7., 14.), (17., 14.)]);
                line(&[(3., 23.), (21., 23.)]);
            }
            Self::Search | Self::Zoom => {
                painter.circle_stroke(point(10., 10.), rect.width() * 0.31, stroke);
                line(&[(15., 15.), (22., 22.)]);
                if self == Self::Zoom {
                    line(&[(6., 10.), (14., 10.)]);
                    line(&[(10., 6.), (10., 14.)]);
                }
            }
            Self::Print => {
                box_at(3., 8., 18., 10.);
                box_at(7., 2., 10., 7.);
                box_at(7., 14., 10., 8.);
            }
            Self::Export => {
                line(&[(3., 4.), (3., 21.), (20., 21.), (20., 16.)]);
                line(&[(8., 15.), (21., 3.), (21., 11.), (21., 3.), (13., 3.)]);
            }
            Self::Settings => {
                painter.circle_stroke(point(12., 12.), rect.width() * 0.31, stroke);
                painter.circle_stroke(point(12., 12.), rect.width() * 0.12, stroke);
                for (x, y, xx, yy) in [
                    (12., 1., 12., 4.),
                    (12., 20., 12., 23.),
                    (1., 12., 4., 12.),
                    (20., 12., 23., 12.),
                    (4., 4., 6., 6.),
                    (18., 18., 20., 20.),
                    (4., 20., 6., 18.),
                    (18., 6., 20., 4.),
                ] {
                    line(&[(x, y), (xx, yy)]);
                }
            }
            Self::Help => {
                painter.circle_stroke(point(12., 12.), rect.width() * 0.43, stroke);
                line(&[
                    (8., 8.),
                    (10., 5.),
                    (15., 5.),
                    (17., 8.),
                    (12., 13.),
                    (12., 15.),
                ]);
                painter.circle_filled(point(12., 19.), 1., color);
            }
            Self::Spark => {
                line(&[
                    (12., 2.),
                    (15., 9.),
                    (22., 12.),
                    (15., 15.),
                    (12., 22.),
                    (9., 15.),
                    (2., 12.),
                    (9., 9.),
                    (12., 2.),
                ]);
            }
            Self::Play => {
                line(&[(7., 3.), (21., 12.), (7., 21.), (7., 3.)]);
            }
            Self::Chevron => {
                line(&[(5., 9.), (12., 16.), (19., 9.)]);
            }
            Self::Camera => {
                line(&[
                    (2., 7.),
                    (7., 7.),
                    (9., 4.),
                    (15., 4.),
                    (17., 7.),
                    (22., 7.),
                    (22., 21.),
                    (2., 21.),
                    (2., 7.),
                ]);
                painter.circle_stroke(point(12., 14.), rect.width() * 0.2, stroke);
            }
            Self::Close => {
                line(&[(5., 5.), (19., 19.)]);
                line(&[(19., 5.), (5., 19.)]);
            }
            Self::Power => {
                line(&[(12., 2.), (12., 13.)]);
                line(&[
                    (6., 5.),
                    (3., 10.),
                    (3., 16.),
                    (7., 21.),
                    (17., 21.),
                    (21., 16.),
                    (21., 10.),
                    (18., 5.),
                ]);
            }
            Self::Home => {
                line(&[
                    (2., 11.),
                    (12., 2.),
                    (22., 11.),
                    (19., 11.),
                    (19., 22.),
                    (5., 22.),
                    (5., 11.),
                    (2., 11.),
                ]);
                box_at(10., 15., 5., 7.);
            }
        }
    }
}

pub fn command_button(
    ui: &mut Ui,
    glyph: Glyph,
    label: &str,
    detail: Detail,
    selected: bool,
) -> (Response, bool) {
    let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
    let palette = Palette::of_ui(ui);
    let size = match detail {
        Detail::Large => vec2(76.0, 78.0),
        Detail::Labeled => vec2(ui.available_width().clamp(88.0, 154.0), 26.0),
        Detail::Icon => vec2(32.0, 30.0),
    };
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, selected);
        let fill = if selected {
            palette.accent_soft
        } else if response.hovered() || response.is_pointer_button_down_on() {
            visuals.bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 4, fill);
        if response.has_focus() && ui.is_enabled() {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                4,
                Stroke::new(2.0, palette.text),
                StrokeKind::Inside,
            );
        }
        let color = if ui.is_enabled() {
            palette.text
        } else {
            ui.visuals().weak_text_color()
        };
        let icon_size = if detail == Detail::Large { 28.0 } else { 19.0 };
        let center = match detail {
            Detail::Large => pos2(rect.center().x, rect.top() + 23.0),
            Detail::Labeled => pos2(rect.left() + 13.0, rect.center().y),
            Detail::Icon => rect.center(),
        };
        glyph.paint(
            ui,
            Rect::from_center_size(center, Vec2::splat(icon_size)),
            if ui.is_enabled() && detail == Detail::Large {
                palette.accent
            } else {
                color
            },
        );
        if detail != Detail::Icon {
            let text_rect = match detail {
                Detail::Large => Rect::from_min_max(
                    pos2(rect.left() + 3., rect.top() + 43.),
                    pos2(rect.right() - 3., rect.bottom() - 2.),
                ),
                _ => Rect::from_min_max(
                    pos2(rect.left() + 28., rect.top() + 3.),
                    pos2(rect.right() - 3., rect.bottom() - 2.),
                ),
            };
            let mut job = egui::text::LayoutJob::simple(
                label.to_owned(),
                FontId::proportional(11.0),
                color,
                text_rect.width(),
            );
            job.wrap.max_rows = if detail == Detail::Large { 2 } else { 1 };
            let galley = ui.painter().layout_job(job);
            let anchor = if detail == Detail::Large {
                Align2::CENTER_TOP
            } else {
                Align2::LEFT_CENTER
            };
            let origin: Pos2 = anchor.align_size_within_rect(galley.size(), text_rect).min;
            ui.painter()
                .with_clip_rect(text_rect.intersect(ui.clip_rect()))
                .galley(origin, galley, color);
        }
    }
    let response = response.on_hover_text(label);
    let keyboard = ui.is_enabled()
        && response.has_focus()
        && ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || input.consume_key(egui::Modifiers::NONE, egui::Key::Space)
        });
    let activated = ui.is_enabled() && ((response.clicked() && touch_allowed) || keyboard);
    (response, activated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::accesskit::Role;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    #[test]
    fn every_detail_preserves_names_keyboard_activation_and_disabled_state() {
        for detail in [Detail::Large, Detail::Labeled, Detail::Icon] {
            let mut h = Harness::builder()
                .with_size(vec2(300., 160.))
                .build_ui_state(
                    move |ui, state: &mut (bool, usize)| {
                        let (_, activated) = ui
                            .add_enabled_ui(state.0, |ui| {
                                command_button(
                                    ui,
                                    Glyph::Down,
                                    "Move selected topics down",
                                    detail,
                                    false,
                                )
                            })
                            .inner;
                        if activated {
                            state.1 += 1;
                        }
                    },
                    (true, 0),
                );
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, "Move selected topics down")
                .click();
            h.run_steps(3);
            assert_eq!(h.state().1, 1);
            h.get_by_role_and_label(Role::Button, "Move selected topics down")
                .focus();
            h.run_steps(2);
            h.key_press(egui::Key::Space);
            h.run_steps(2);
            h.key_press(egui::Key::Enter);
            h.run_steps(2);
            assert_eq!(h.state().1, 3);
            h.state_mut().0 = false;
            h.run_steps(3);
            assert!(
                h.get_by_role_and_label(Role::Button, "Move selected topics down")
                    .accesskit_node()
                    .is_disabled()
            );
            h.key_press(egui::Key::Enter);
            h.run_steps(3);
            assert_eq!(h.state().1, 3);
        }
    }

    #[test]
    fn canceled_touch_cannot_activate_a_ribbon_command() {
        let mut h = Harness::builder().build_ui_state(
            |ui, count: &mut usize| {
                if command_button(ui, Glyph::Delete, "Delete", Detail::Icon, false).1 {
                    *count += 1;
                }
            },
            0,
        );
        h.run_steps(3);
        let pos = h
            .get_by_role_and_label(Role::Button, "Delete")
            .rect()
            .center();
        h.event(egui::Event::PointerMoved(pos));
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        for phase in [egui::TouchPhase::Start, egui::TouchPhase::Cancel] {
            h.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(1),
                phase,
                pos,
                force: None,
            });
            h.run_steps(2);
        }
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(3);
        assert_eq!(*h.state(), 0);
        h.get_by_role_and_label(Role::Button, "Delete").click();
        h.run_steps(3);
        assert_eq!(*h.state(), 1);
    }
}
