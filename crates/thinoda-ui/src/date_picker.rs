//! A typed ISO date field with a calendar popup (UR3).
//!
//! Keyboard users keep typing `YYYY-MM-DD`; pointer and touch users open a month grid
//! from the calendar button. Days rejected by the caller's availability rule (for
//! example non-working days of a work week) are shown muted and cannot be picked.

use egui::{
    Align2, CornerRadius, FontId, Id, Key, Popup, PopupCloseBehavior, Response, Role, Sense,
    Stroke, StrokeKind, TextEdit, Ui, Vec2, WidgetInfo, vec2,
};

use crate::icons;
use crate::tokens::{Palette, Tokens};

/// A proleptic Gregorian calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl CalendarDate {
    /// A valid date, or `None` for impossible combinations such as February 30.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        ((1..=9999).contains(&year)
            && (1..=12).contains(&month)
            && day >= 1
            && day <= days_in_month(year, month))
        .then_some(Self { year, month, day })
    }

    /// Parses `YYYY-MM-DD`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().split('-');
        let year = parts.next()?;
        let month = parts.next()?;
        let day = parts.next()?;
        if parts.next().is_some()
            || year.len() != 4
            || month.len() != 2
            || day.len() != 2
            || ![year, month, day]
                .iter()
                .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return None;
        }
        Self::new(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)
    }

    /// The civil date `days` after 1970-01-01 (negative values are earlier).
    pub fn from_days_since_epoch(days: i64) -> Self {
        // Howard Hinnant's civil_from_days.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Self { year, month, day }
    }

    /// Days since 1970-01-01.
    pub fn days_since_epoch(self) -> i64 {
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let yoe = year.rem_euclid(400);
        let month = i64::from(self.month);
        let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5
            + i64::from(self.day)
            - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Weekday with Monday = 0 … Sunday = 6.
    pub fn weekday_from_monday(self) -> usize {
        // 1970-01-01 was a Thursday (3).
        (self.days_since_epoch() + 3).rem_euclid(7) as usize
    }

    /// The same day shifted by `days`.
    pub fn add_days(self, days: i64) -> Self {
        Self::from_days_since_epoch(self.days_since_epoch() + days)
    }
}

impl std::fmt::Display for CalendarDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Days in `month` (1-12) of `year`.
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

/// Translated texts for [`date_field`].
#[derive(Clone, Debug)]
pub struct CalendarLabels {
    /// January … December.
    pub months: [String; 12],
    /// Short weekday names, Monday first.
    pub weekdays: [String; 7],
    pub previous_month: String,
    pub next_month: String,
    pub today: String,
    pub open_calendar: String,
    pub unavailable: String,
    /// First column of the grid, Monday = 0.
    pub first_weekday: usize,
}

impl Default for CalendarLabels {
    fn default() -> Self {
        let months = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        let weekdays = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
        Self {
            months: months.map(String::from),
            weekdays: weekdays.map(String::from),
            previous_month: "Previous month".into(),
            next_month: "Next month".into(),
            today: "Today".into(),
            open_calendar: "Open calendar".into(),
            unavailable: "Non-working day".into(),
            first_weekday: 0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct ViewMonth {
    year: i32,
    month: u8,
    focus: Option<CalendarDate>,
}

/// Outcome of one [`date_field`] frame.
pub struct DateFieldResponse {
    /// The text edit response (label it with `labelled_by`).
    pub text: Response,
    /// The calendar toggle button.
    pub button: Response,
    /// Set when a day was picked from the calendar this frame.
    pub picked: Option<CalendarDate>,
}

/// A `YYYY-MM-DD` text field followed by a calendar button. Picking a day writes it into
/// `text`; `available` greys out and blocks days such as non-working days.
pub fn date_field(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    labels: &CalendarLabels,
    today: Option<CalendarDate>,
    available: impl Fn(CalendarDate) -> bool,
) -> DateFieldResponse {
    crate::fonts::ensure_icon_font(ui.ctx());
    let id = ui.make_persistent_id(id_salt);
    let touch_allowed = crate::widgets::touch_activation_allowed(ui.ctx());
    let mut picked = None;
    let inner = ui.horizontal(|ui| {
        let text_response = ui.add(
            TextEdit::singleline(text)
                .hint_text("YYYY-MM-DD")
                .desired_width(110.0),
        );
        let button = crate::widgets::icon_button(ui, icons::CALENDAR, &labels.open_calendar, false);
        if button.clicked() && touch_allowed {
            let base = CalendarDate::parse(text).or(today).unwrap_or(CalendarDate {
                year: 2000,
                month: 1,
                day: 1,
            });
            ui.data_mut(|data| {
                data.insert_temp(
                    id,
                    ViewMonth {
                        year: base.year,
                        month: base.month,
                        focus: Some(base),
                    },
                )
            });
        }
        let mut popup = Popup::menu(&button)
            .id(id.with("popup"))
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside);
        if !touch_allowed {
            popup = popup.open_memory(None);
        }
        popup.show(|ui| {
            let mut view = ui
                .data(|data| data.get_temp::<ViewMonth>(id))
                .unwrap_or_default();
            if view.month == 0 {
                let base = CalendarDate::parse(text).or(today).unwrap_or(CalendarDate {
                    year: 2000,
                    month: 1,
                    day: 1,
                });
                view = ViewMonth {
                    year: base.year,
                    month: base.month,
                    focus: Some(base),
                };
            }
            let selected = CalendarDate::parse(text);
            let max_height = (ui.ctx().content_rect().height() - 32.0).max(0.0);
            let chosen = egui::ScrollArea::vertical()
                .max_height(max_height)
                .show(ui, |ui| {
                    month_grid(
                        ui,
                        id,
                        &mut view,
                        labels,
                        selected,
                        today,
                        &available,
                        touch_allowed,
                    )
                })
                .inner;
            if let Some(date) = chosen {
                picked = Some(date);
                ui.close();
            }
            ui.data_mut(|data| data.insert_temp(id, view));
        });
        (text_response, button)
    });
    let (mut text_response, button) = inner.inner;
    if let Some(date) = picked {
        *text = date.to_string();
        text_response.mark_changed();
        button.request_focus();
    }
    DateFieldResponse {
        text: text_response,
        button,
        picked,
    }
}

#[expect(clippy::too_many_arguments)]
fn month_grid(
    ui: &mut Ui,
    id: Id,
    view: &mut ViewMonth,
    labels: &CalendarLabels,
    selected: Option<CalendarDate>,
    today: Option<CalendarDate>,
    available: &impl Fn(CalendarDate) -> bool,
    touch_allowed: bool,
) -> Option<CalendarDate> {
    let p = Palette::of_ui(ui);
    let cell = Tokens::ICON_BUTTON + 4.0;
    let mut picked = None;
    // Arrow keys move among available days; Page Up/Down keep the day where possible.
    let focused = ui.memory(|memory| memory.focused());
    if let Some(current) = (1..=days_in_month(view.year, view.month))
        .filter_map(|day| CalendarDate::new(view.year, view.month, day))
        .find(|date| focused == Some(id.with(date)))
    {
        let change = ui.input_mut(|input| {
            [
                (Key::ArrowLeft, -1),
                (Key::ArrowRight, 1),
                (Key::ArrowUp, -7),
                (Key::ArrowDown, 7),
            ]
            .into_iter()
            .find_map(|(key, days)| {
                input
                    .consume_key(egui::Modifiers::NONE, key)
                    .then_some(days)
            })
        });
        if let Some(days) = change {
            let mut next = current.add_days(days);
            for _ in 0..366 {
                if CalendarDate::new(next.year, next.month, next.day).is_none() {
                    break;
                }
                if available(next) {
                    view.year = next.year;
                    view.month = next.month;
                    view.focus = Some(next);
                    break;
                }
                next = next.add_days(days.signum());
            }
        }
        let month_delta = ui.input_mut(|input| {
            if input.consume_key(egui::Modifiers::NONE, Key::PageUp) {
                -1
            } else if input.consume_key(egui::Modifiers::NONE, Key::PageDown) {
                1
            } else {
                0
            }
        });
        if month_delta != 0 {
            shift_month(view, month_delta);
            view.focus = CalendarDate::new(
                view.year,
                view.month,
                current.day.min(days_in_month(view.year, view.month)),
            );
        }
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled_ui(view.year > 1 || view.month > 1, |ui| {
                crate::widgets::icon_button(ui, icons::ARROW_LEFT, &labels.previous_month, false)
            })
            .inner
            .clicked()
            && touch_allowed
        {
            shift_month(view, -1);
        }
        let title = format!(
            "{} {}",
            labels.months[usize::from(view.month - 1)],
            view.year
        );
        let width = cell * 7.0 - Tokens::ICON_BUTTON * 2.0 - ui.spacing().item_spacing.x * 2.0;
        ui.add_sized(
            vec2(width.max(80.0), Tokens::ICON_BUTTON),
            egui::Label::new(egui::RichText::new(title).strong()),
        );
        if ui
            .add_enabled_ui(view.year < 9999 || view.month < 12, |ui| {
                crate::widgets::icon_button(ui, icons::ARROW_RIGHT, &labels.next_month, false)
            })
            .inner
            .clicked()
            && touch_allowed
        {
            shift_month(view, 1);
        }
    });
    let first = CalendarDate {
        year: view.year,
        month: view.month,
        day: 1,
    };
    let offset = (first.weekday_from_monday() + 7 - labels.first_weekday % 7) % 7;
    let days = days_in_month(view.year, view.month);
    if let Some(focus) = view.focus
        && !available(focus)
    {
        view.focus = (focus.day..=days)
            .chain(1..focus.day)
            .filter_map(|day| CalendarDate::new(view.year, view.month, day))
            .find(|date| available(*date));
    }
    egui::Grid::new(ui.scope_id().with(("calendar", view.year, view.month)))
        .spacing(Vec2::ZERO)
        .min_col_width(cell)
        .show(ui, |ui| {
            for column in 0..7 {
                let weekday = (labels.first_weekday + column) % 7;
                ui.add_sized(
                    vec2(cell, cell * 0.75),
                    egui::Label::new(
                        egui::RichText::new(&labels.weekdays[weekday])
                            .size(Tokens::FONT_SMALL)
                            .color(p.text_muted),
                    ),
                );
            }
            ui.end_row();
            let mut column = 0;
            for _ in 0..offset {
                ui.allocate_exact_size(Vec2::splat(cell), Sense::hover());
                column += 1;
            }
            for day in 1..=days {
                let date = CalendarDate {
                    year: view.year,
                    month: view.month,
                    day,
                };
                let enabled = available(date);
                if day_cell(
                    ui,
                    id.with(date),
                    cell,
                    date,
                    enabled,
                    selected == Some(date),
                    today == Some(date),
                    labels,
                    view.focus == Some(date),
                )
                .clicked()
                    && enabled
                    && touch_allowed
                {
                    picked = Some(date);
                }
                column += 1;
                if column % 7 == 0 {
                    ui.end_row();
                }
            }
        });
    view.focus = None;
    if let Some(today) = today
        && ui
            .add_enabled(available(today), egui::Button::new(&labels.today))
            .clicked()
        && touch_allowed
    {
        picked = Some(today);
    }
    picked
}

fn shift_month(view: &mut ViewMonth, delta: i32) {
    let month = (view.year * 12 + i32::from(view.month) - 1 + delta).clamp(12, 9999 * 12 + 11);
    view.year = month / 12;
    view.month = (month % 12 + 1) as u8;
}

#[expect(clippy::too_many_arguments)]
fn day_cell(
    ui: &mut Ui,
    id: Id,
    size: f32,
    date: CalendarDate,
    enabled: bool,
    selected: bool,
    today: bool,
    labels: &CalendarLabels,
    focus: bool,
) -> Response {
    let p = Palette::of_ui(ui);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let response = ui.interact(rect, id, sense);
    if focus && enabled {
        response.request_focus();
    }
    let label = date.to_string();
    response.widget_info(|| {
        let mut info = WidgetInfo::selected(Role::Button, enabled, selected, &label);
        if !enabled {
            info.label = Some(format!("{label}, {}", labels.unavailable));
        }
        info
    });
    if ui.is_rect_visible(rect) {
        let inner = rect.shrink(2.0);
        if selected {
            ui.painter()
                .rect_filled(inner, CornerRadius::same(Tokens::RADIUS_S), p.accent);
        } else if enabled && response.hovered() {
            ui.painter()
                .rect_filled(inner, CornerRadius::same(Tokens::RADIUS_S), p.surface_alt);
        }
        if today && !selected {
            ui.painter().rect_stroke(
                inner,
                CornerRadius::same(Tokens::RADIUS_S),
                Stroke::new(1.0, p.accent),
                StrokeKind::Inside,
            );
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                inner,
                CornerRadius::same(Tokens::RADIUS_S),
                Stroke::new(2.0, p.text),
                StrokeKind::Inside,
            );
        }
        let color = if selected {
            p.accent_text
        } else if enabled {
            p.text
        } else {
            p.text_muted
        };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            date.day.to_string(),
            FontId::proportional(Tokens::FONT_BODY),
            color,
        );
        if !enabled {
            // A strike makes unavailable days distinguishable without relying on color.
            let y = rect.center().y;
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left() + 9.0, y),
                    egui::pos2(rect.right() - 9.0, y),
                ],
                Stroke::new(1.0, p.text_muted.gamma_multiply(0.55)),
            );
        }
    }
    response.on_hover_text(if enabled {
        String::new()
    } else {
        labels.unavailable.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn civil_conversion_roundtrips_and_knows_weekdays() {
        for days in [-800_000_i64, -1, 0, 1, 10_957, 19_000, 2_000_000] {
            let date = CalendarDate::from_days_since_epoch(days);
            assert_eq!(date.days_since_epoch(), days, "{date}");
        }
        assert_eq!(
            CalendarDate::from_days_since_epoch(0).to_string(),
            "1970-01-01"
        );
        // 2024-02-29 was a Thursday; 2026-10-26 is a Monday.
        assert_eq!(
            CalendarDate::parse("2024-02-29")
                .unwrap()
                .weekday_from_monday(),
            3
        );
        assert_eq!(
            CalendarDate::parse("2026-10-26")
                .unwrap()
                .weekday_from_monday(),
            0
        );
        assert_eq!(
            CalendarDate::parse("2024-12-31")
                .unwrap()
                .add_days(1)
                .to_string(),
            "2025-01-01"
        );
    }

    #[test]
    fn parse_rejects_malformed_and_impossible_dates() {
        for bad in [
            "",
            "2024-2-01",
            "2023-02-29",
            "2024-13-01",
            "2024-01-32",
            "20240101",
            "2024-01-01-1",
            "+024-01-01",
            "0000-01-01",
        ] {
            assert!(CalendarDate::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(
            CalendarDate::parse(" 2024-02-29 "),
            CalendarDate::new(2024, 2, 29)
        );
    }

    #[test]
    fn month_navigation_stays_in_supported_years() {
        let mut view = ViewMonth {
            year: 1,
            month: 1,
            focus: None,
        };
        shift_month(&mut view, -1);
        assert_eq!((view.year, view.month), (1, 1));
        view.year = 9999;
        view.month = 12;
        shift_month(&mut view, 1);
        assert_eq!((view.year, view.month), (9999, 12));
        shift_month(&mut view, -1);
        assert_eq!((view.year, view.month), (9999, 11));
    }

    #[test]
    fn keyboard_skips_unavailable_dates_and_commits_with_enter() {
        let mut text = String::from("2026-10-16");
        let labels = CalendarLabels::default();
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            date_field(ui, "start", &mut text, &labels, None, |date| {
                date.weekday_from_monday() < 5
            });
        });
        harness.get_by_label(&labels.open_calendar).click();
        harness.run();
        harness.key_press(Key::ArrowRight);
        harness.run();
        harness.key_press(Key::Enter);
        harness.run();
        drop(harness);
        assert_eq!(text, "2026-10-19");
    }

    #[test]
    fn canceled_touch_cannot_open_calendar() {
        let mut text = String::from("2026-10-16");
        let labels = CalendarLabels::default();
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            date_field(ui, "start", &mut text, &labels, None, |_| true);
        });
        let pos = harness.get_by_label(&labels.open_calendar).rect().center();
        for (phase, pressed) in [
            (egui::TouchPhase::Start, true),
            (egui::TouchPhase::Cancel, false),
        ] {
            harness.event(egui::Event::PointerMoved(pos));
            harness.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(1),
                phase,
                pos,
                force: None,
            });
            harness.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run_steps(1);
        }
        harness.run_steps(2);
        assert!(!Popup::is_any_open(&harness.ctx));
        drop(harness);
        assert_eq!(text, "2026-10-16");
    }

    #[test]
    fn picking_a_day_fills_the_field_and_unavailable_days_are_blocked() {
        let mut text = String::from("2026-10-14");
        let labels = CalendarLabels::default();
        let weekdays_only = |date: CalendarDate| date.weekday_from_monday() < 5;
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            let _ = date_field(ui, "due", &mut text, &labels, None, weekdays_only);
        });
        harness.get_by_label(&labels.open_calendar).click();
        harness.run();
        // 2026-10-17 is a Saturday: disabled, no change.
        harness
            .get_by_label(&format!("2026-10-17, {}", labels.unavailable))
            .click();
        harness.run();
        harness.get_by_label("2026-10-16").click();
        harness.run();
        drop(harness);
        assert_eq!(text, "2026-10-16");
    }
}
