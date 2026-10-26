//! Adaptive ribbon command controls using the shared Material Symbols vocabulary.

use egui::{Align2, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::{Palette, icons, widgets};

/// A command icon: one [`icons`] glyph, an optional small corner badge (also an [`icons`]
/// glyph) for variants Material Symbols has no dedicated icon for, and a semantic tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CommandIcon {
    pub symbol: &'static str,
    pub badge: Option<&'static str>,
    pub tone: IconTone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconTone {
    Neutral,
    File,
    Edit,
    Create,
    Format,
    Ai,
    View,
    Help,
    Danger,
}

impl IconTone {
    pub fn color(self, dark: bool, primary: bool) -> egui::Color32 {
        let (light_primary, light_secondary, dark_primary, dark_secondary) = match self {
            Self::Neutral | Self::View => (0x236baa, 0x526478, 0x86bcf1, 0xb0bdcf),
            Self::File => (0x005ea6, 0x00716b, 0x73b8ff, 0x56d7cc),
            Self::Edit => (0x4f46b9, 0x2569a8, 0xafa4ff, 0x7eb6ef),
            Self::Create => (0x237140, 0x007a76, 0x79d79a, 0x5bcfc8),
            Self::Format => (0x8040a5, 0xa13c75, 0xd8a1f3, 0xeca0cb),
            Self::Ai => (0x703bd0, 0x007576, 0xcba8ff, 0x5ed6d1),
            Self::Help => (0x00757e, 0x2b68a7, 0x6dd2d6, 0x91bbe8),
            Self::Danger => (0xb42330, 0xa54a00, 0xff8e99, 0xffc086),
        };
        let rgb = match (dark, primary) {
            (false, true) => light_primary,
            (false, false) => light_secondary,
            (true, true) => dark_primary,
            (true, false) => dark_secondary,
        };
        egui::Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }
}

impl From<&'static str> for CommandIcon {
    fn from(symbol: &'static str) -> Self {
        Self {
            symbol,
            badge: None,
            tone: IconTone::Neutral,
        }
    }
}

impl CommandIcon {
    pub const fn marked(symbol: &'static str, badge: &'static str) -> Self {
        Self {
            symbol,
            badge: Some(badge),
            tone: IconTone::Neutral,
        }
    }

    pub const fn with_tone(mut self, tone: IconTone) -> Self {
        self.tone = tone;
        self
    }

    /// Paints the icon to fill `rect` (a square of one of the [`icons`] size tiers).
    pub fn paint(self, ui: &Ui, rect: Rect, color: egui::Color32) {
        paint_symbol(ui, self.symbol, rect, color);
        if let Some(badge) = self.badge {
            let badge_rect =
                Rect::from_min_max(rect.min + rect.size() * 0.5, rect.max + vec2(1.0, 1.0));
            ui.painter().circle_filled(
                badge_rect.center(),
                badge_rect.width() * 0.55,
                Palette::of_ui(ui).surface,
            );
            paint_symbol(ui, badge, badge_rect, color);
        }
    }
}

/// Paints one icon glyph centered in `rect`, sized to the rect height.
pub fn paint_symbol(ui: &Ui, symbol: &str, rect: Rect, color: egui::Color32) {
    if !crate::fonts::ensure_icon_symbol(ui.ctx(), symbol) {
        return;
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        symbol,
        FontId::proportional(rect.height()),
        color,
    );
}

/// A menu row with a leading [`icons::SIZE_SMALL`] command icon slot (empty for
/// `None`, so labels stay aligned) and an optional shortcut. The accessible name is
/// the label followed by the shortcut, as for plain egui menu buttons.
pub fn menu_item(
    ui: &mut Ui,
    icon: Option<CommandIcon>,
    label: &str,
    shortcut: Option<String>,
) -> Response {
    let slot = egui::IdSalt::new("thinoda-menu-icon");
    let mut button = egui::Button::new((
        egui::Atom::custom(slot, Vec2::splat(icons::SIZE_SMALL)),
        label,
    ));
    if let Some(shortcut) = shortcut {
        button = button.shortcut_text(shortcut);
    }
    let response = button.atom_ui(ui);
    if let (Some(icon), Some(rect)) = (icon, response.rect(slot)) {
        let color = if ui.is_enabled() {
            icon.tone.color(ui.visuals().dark_mode, true)
        } else {
            ui.visuals().weak_text_color()
        };
        icon.paint(ui, rect, color);
    }
    response.response
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Large,
    Labeled,
    Icon,
    CompactLarge,
    CompactLabeled,
    CompactIcon,
}

impl Detail {
    pub fn compact(self) -> Self {
        match self {
            Self::Large | Self::CompactLarge => Self::CompactLarge,
            Self::Labeled | Self::CompactLabeled => Self::CompactLabeled,
            Self::Icon | Self::CompactIcon => Self::CompactIcon,
        }
    }

    fn is_compact(self) -> bool {
        matches!(
            self,
            Self::CompactLarge | Self::CompactLabeled | Self::CompactIcon
        )
    }

    fn icon_size(self) -> f32 {
        match self {
            Self::Large => 32.0,
            Self::Labeled => 24.0,
            Self::Icon => 16.0,
            Self::CompactLarge => 24.0,
            Self::CompactLabeled => 20.0,
            Self::CompactIcon => 16.0,
        }
    }
}

pub fn command_button(
    ui: &mut Ui,
    icon: impl Into<CommandIcon>,
    label: &str,
    detail: Detail,
    selected: bool,
    tooltip: Option<&str>,
) -> (Response, bool) {
    button(ui, icon.into(), label, detail, selected, tooltip, false)
}

pub fn menu_button(
    ui: &mut Ui,
    icon: impl Into<CommandIcon>,
    label: &str,
    detail: Detail,
    tooltip: Option<&str>,
) -> (Response, bool) {
    button(ui, icon.into(), label, detail, false, tooltip, true)
}

fn button(
    ui: &mut Ui,
    icon: CommandIcon,
    label: &str,
    detail: Detail,
    selected: bool,
    tooltip: Option<&str>,
    dropdown: bool,
) -> (Response, bool) {
    let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
    let palette = Palette::of_ui(ui);
    let size = match detail {
        Detail::Large => vec2(112.0, 96.0),
        Detail::Labeled => vec2(ui.available_width().clamp(112.0, 260.0), 44.0),
        Detail::Icon => vec2(44.0, 44.0),
        Detail::CompactLarge => vec2(104.0, 72.0),
        Detail::CompactLabeled => vec2(ui.available_width().clamp(104.0, 240.0), 34.0),
        Detail::CompactIcon => vec2(34.0, 34.0),
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
        let icon_size = detail.icon_size();
        let center = match detail {
            Detail::Large => pos2(rect.center().x, rect.top() + 26.0),
            Detail::Labeled => pos2(rect.left() + 20.0, rect.center().y),
            Detail::Icon => rect.center(),
            Detail::CompactLarge => pos2(rect.center().x, rect.top() + 18.0),
            Detail::CompactLabeled => pos2(rect.left() + 16.0, rect.center().y),
            Detail::CompactIcon => rect.center(),
        };
        let icon_color = if ui.is_enabled() {
            icon.tone.color(
                palette.dark,
                matches!(detail, Detail::Large | Detail::CompactLarge),
            )
        } else {
            ui.visuals().weak_text_color()
        };
        icon.paint(
            ui,
            Rect::from_center_size(center, Vec2::splat(icon_size)),
            icon_color,
        );
        if !matches!(detail, Detail::Icon | Detail::CompactIcon) {
            let text_rect = match detail {
                Detail::Large => Rect::from_min_max(
                    pos2(rect.left() + 8., rect.top() + 48.),
                    pos2(
                        rect.right() - if dropdown { 24. } else { 8. },
                        rect.bottom() - 8.,
                    ),
                ),
                Detail::CompactLarge => Rect::from_min_max(
                    pos2(rect.left() + 6., rect.top() + 34.),
                    pos2(
                        rect.right() - if dropdown { 24. } else { 6. },
                        rect.bottom() - 4.,
                    ),
                ),
                _ => Rect::from_min_max(
                    pos2(
                        rect.left() + if detail.is_compact() { 32. } else { 40. },
                        rect.top() + if detail.is_compact() { 4. } else { 8. },
                    ),
                    pos2(
                        rect.right() - if dropdown { 28. } else { 8. },
                        rect.bottom() - if detail.is_compact() { 4. } else { 8. },
                    ),
                ),
            };
            let mut job = egui::text::LayoutJob::simple(
                label.to_owned(),
                FontId::proportional(12.0),
                color,
                text_rect.width(),
            );
            job.wrap.max_rows = if matches!(detail, Detail::Large | Detail::CompactLarge) {
                2
            } else {
                1
            };
            let galley = ui.painter().layout_job(job);
            let anchor = if matches!(detail, Detail::Large | Detail::CompactLarge) {
                Align2::CENTER_TOP
            } else {
                Align2::LEFT_CENTER
            };
            let origin: Pos2 = anchor.align_size_within_rect(galley.size(), text_rect).min;
            if dropdown {
                paint_symbol(
                    ui,
                    icons::CHEVRON_DOWN,
                    Rect::from_center_size(
                        pos2(rect.right() - 14., origin.y + galley.size().y * 0.5),
                        Vec2::splat(13.0),
                    ),
                    color,
                );
            }
            ui.painter()
                .with_clip_rect(text_rect.intersect(ui.clip_rect()))
                .galley(origin, galley, color);
        } else if dropdown {
            paint_symbol(
                ui,
                icons::CHEVRON_DOWN,
                Rect::from_center_size(pos2(rect.right() - 7., rect.center().y), Vec2::splat(13.0)),
                color,
            );
        }
    }
    let response = response.on_hover_text(tooltip.unwrap_or(label));
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
    fn semantic_colors_differ_by_priority_and_contrast_in_both_themes() {
        fn luminance(color: egui::Color32) -> f32 {
            let channel = |value: u8| {
                let value = value as f32 / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
        }
        for dark in [false, true] {
            let palette = if dark {
                Palette::dark(crate::tokens::DEFAULT_ACCENT)
            } else {
                Palette::light(crate::tokens::DEFAULT_ACCENT)
            };
            for tone in [
                IconTone::Neutral,
                IconTone::File,
                IconTone::Edit,
                IconTone::Create,
                IconTone::Format,
                IconTone::Ai,
                IconTone::View,
                IconTone::Help,
                IconTone::Danger,
            ] {
                assert_ne!(tone.color(dark, true), tone.color(dark, false));
                for primary in [false, true] {
                    for background in [palette.surface, palette.surface_alt, palette.accent_soft] {
                        let foreground = luminance(tone.color(dark, primary));
                        let alpha = background.a() as f32 / 255.0;
                        let composite = |front: u8, back: u8| {
                            (front as f32 + back as f32 * (1.0 - alpha))
                                .round()
                                .min(255.0) as u8
                        };
                        let background = luminance(egui::Color32::from_rgb(
                            composite(background.r(), palette.surface.r()),
                            composite(background.g(), palette.surface.g()),
                            composite(background.b(), palette.surface.b()),
                        ));
                        let contrast = (foreground.max(background) + 0.05)
                            / (foreground.min(background) + 0.05);
                        assert!(
                            contrast >= 3.0,
                            "{tone:?}, dark={dark}, primary={primary}: {contrast}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn dropdown_chevrons_align_with_their_caption() {
        for detail in [
            Detail::Large,
            Detail::Labeled,
            Detail::CompactLarge,
            Detail::CompactLabeled,
        ] {
            let mut h = Harness::builder()
                .with_size(vec2(300., 180.))
                .build_ui(move |ui| {
                    menu_button(ui, icons::OPEN, "Open Recent", detail, None);
                });
            h.run_steps(3);
            let rect = h.get_by_role_and_label(Role::Button, "Open Recent").rect();
            let text = h
                .output()
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "Open Recent" => Some(text),
                    _ => None,
                })
                .unwrap();
            let arrow = h
                .output()
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.job.text == icons::CHEVRON_DOWN
                            && text.visual_bounding_rect().center().x > rect.right() - 22. =>
                    {
                        Some(text.visual_bounding_rect())
                    }
                    _ => None,
                })
                .expect("dropdown has one caption-adjacent chevron");
            let offset = arrow.center().y - (text.pos.y + text.galley.size().y * 0.5);
            assert!(offset.abs() < 1.5, "{detail:?}: {offset}");
            assert!(arrow.left() >= text.pos.x + text.galley.size().x);
        }
    }

    #[test]
    fn command_icons_use_consistent_priority_tiers_and_touch_targets() {
        let mut sizes = Vec::new();
        for detail in [
            Detail::Large,
            Detail::Labeled,
            Detail::Icon,
            Detail::CompactLarge,
            Detail::CompactLabeled,
            Detail::CompactIcon,
        ] {
            let mut h = Harness::builder()
                .with_size(vec2(300., 160.))
                .build_ui(move |ui| {
                    command_button(ui, icons::OPEN, "Open", detail, false, None);
                });
            h.run_steps(3);
            let rect = h.get_by_role_and_label(Role::Button, "Open").rect();
            let minimum = if detail.is_compact() { 34.0 } else { 44.0 };
            assert!(rect.width() >= minimum && rect.height() >= minimum);
            let font_size = h
                .output()
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == icons::OPEN => {
                        Some(text.galley.job.sections[0].format.font_id.size)
                    }
                    _ => None,
                })
                .expect("command icon glyph");
            sizes.push(font_size / detail.icon_size());
        }
        for size in &sizes[1..] {
            assert!((*size - sizes[0]).abs() < 0.01, "{sizes:?}");
        }
    }

    #[test]
    fn every_detail_preserves_names_keyboard_activation_and_disabled_state() {
        for detail in [
            Detail::Large,
            Detail::Labeled,
            Detail::Icon,
            Detail::CompactLarge,
            Detail::CompactLabeled,
            Detail::CompactIcon,
        ] {
            let mut h = Harness::builder()
                .with_size(vec2(300., 160.))
                .build_ui_state(
                    move |ui, state: &mut (bool, usize)| {
                        let (_, activated) = ui
                            .add_enabled_ui(state.0, |ui| {
                                command_button(
                                    ui,
                                    icons::MOVE_DOWN,
                                    "Move selected topics down",
                                    detail,
                                    false,
                                    None,
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
                if command_button(ui, icons::TRASH, "Delete", Detail::Icon, false, None).1 {
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
