//! Dialogs: the shared resizable dialog frame ([`Dialog`]), icon tab headers
//! ([`tab_bar`]) and small confirmation prompts ([`confirm_dialog`]).
//!
//! Every Thinoda dialog uses [`Dialog`] so About, Help, Settings, Gantt and the other
//! tools look and behave the same: a title row with an optional icon, Hide (collapse
//! to the title row), Maximize/Restore (also by double-clicking the title) and Close
//! buttons, resizing from all four edges and corners with a visible lower-right
//! grip, a vertical scrollbar when content is taller than the window and bounds that
//! keep the dialog inside the app window.

use crate::tokens::{Palette, Tokens};
use egui::{
    Align, Align2, Color32, Context, CornerRadius, FontId, Frame, Id, Key, LayerId, Layout, Order,
    Pos2, Rect, Response, RichText, Role, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, vec2,
};

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
    confirm_dialog_with_labels(ctx, id, title, message, buttons, &DialogLabels::default())
}

/// A confirmation prompt using the shared dialog frame and localized caption controls.
pub fn confirm_dialog_with_labels(
    ctx: &Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    title: &str,
    message: &str,
    buttons: &[(&str, DialogButton)],
    labels: &DialogLabels,
) -> Option<usize> {
    let p = Palette::of(ctx);
    let touch_allowed = crate::widgets::touch_activation_allowed(ctx);
    let mut pressed = None;
    let resp = Dialog::new(Id::unique(id), title)
        .labels(labels)
        .modal(true)
        .close_on_backdrop(true)
        .default_width(440.0)
        .fit_content_height(true)
        .show(ctx, |ui| {
            ui.label(RichText::new(message).color(p.text_muted));
            ui.add_space(12.0);
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), ui.spacing().interact_size.y.max(28.0)),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    for (i, (label, kind)) in buttons.iter().enumerate().rev() {
                        let r = match kind {
                            DialogButton::Primary => crate::widgets::primary_button(ui, label),
                            DialogButton::Secondary => crate::widgets::secondary_button(ui, label),
                            DialogButton::Danger => ui.add(
                                egui::Button::new(
                                    RichText::new(*label).color(egui::Color32::WHITE),
                                )
                                .fill(p.danger)
                                .min_size(egui::vec2(80.0, 28.0)),
                            ),
                        };
                        if r.clicked() && touch_allowed {
                            pressed = Some(i);
                        }
                    }
                },
            );
        });
    if pressed.is_none() && resp.close_requested && touch_allowed {
        return Some(usize::MAX);
    }
    pressed
}

/// Translated tooltips/accessible names for the dialog title-row buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogLabels {
    pub hide: String,
    pub show: String,
    pub maximize: String,
    pub restore: String,
    pub close: String,
}

impl Default for DialogLabels {
    fn default() -> Self {
        Self {
            hide: "Minimize".into(),
            show: "Expand".into(),
            maximize: "Maximize".into(),
            restore: "Restore".into(),
            close: "Close".into(),
        }
    }
}

/// Window state shared across frames, keyed by the dialog id.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DialogState {
    /// Fills the app window.
    pub maximized: bool,
    /// Collapsed to its title row.
    pub hidden: bool,
    /// The user resized the dialog; the content then scrolls inside the chosen size.
    pub user_sized: bool,
    /// Outer rectangle before maximizing.
    restore: Option<Rect>,
    /// Re-apply `restore` this frame.
    restore_pending: bool,
    /// Last outer rectangle.
    rect: Option<Rect>,
}

impl DialogState {
    fn key(id: Id) -> Id {
        id.with("thinoda_dialog_state")
    }

    /// The stored state of the dialog `id`.
    pub fn load(ctx: &Context, id: Id) -> Self {
        ctx.data(|data| data.get_temp(Self::key(id)))
            .unwrap_or_default()
    }

    fn store(self, ctx: &Context, id: Id) {
        ctx.data_mut(|data| data.insert_temp(Self::key(id), self));
    }
}

/// Result of [`Dialog::show`].
pub struct DialogResponse<R> {
    /// The content's return value; `None` while hidden.
    pub inner: Option<R>,
    /// The Close button was pressed, or Esc/backdrop dismissed a modal dialog.
    pub close_requested: bool,
    /// Outer rectangle of the dialog this frame.
    pub rect: Option<Rect>,
    /// Layer of the dialog this frame.
    pub layer: LayerId,
    /// Current maximize state, including title-row actions.
    pub maximized: bool,
}

/// Distance kept between a dialog and the edge of the app window.
pub const DIALOG_INSET: f32 = 8.0;
/// Height of the title row.
pub const TITLE_ROW_HEIGHT: f32 = 28.0;

/// A titled, resizable, maximizable dialog window. See the module documentation.
#[must_use = "call .show()"]
pub struct Dialog<'a> {
    id: Id,
    title: &'a str,
    icon: Option<&'a str>,
    labels: Option<&'a DialogLabels>,
    modal: bool,
    close_on_backdrop: bool,
    default_size: Vec2,
    fit_content_height: bool,
    min_size: Vec2,
    resizable: bool,
    scroll: bool,
    hscroll: bool,
    enabled: bool,
    open: Option<&'a mut bool>,
    maximized: Option<bool>,
}

impl<'a> Dialog<'a> {
    /// A dialog whose state is keyed by `id` and whose title row shows `title`.
    pub fn new(id: Id, title: &'a str) -> Self {
        Self {
            id,
            title,
            icon: None,
            labels: None,
            modal: false,
            close_on_backdrop: false,
            default_size: vec2(480.0, 0.0),
            fit_content_height: false,
            min_size: vec2(240.0, 96.0),
            resizable: true,
            scroll: true,
            hscroll: false,
            enabled: true,
            open: None,
            maximized: None,
        }
    }

    /// Sets `*open = false` when closing is requested, like [`egui::Window::open`].
    pub fn open(mut self, open: &'a mut bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Synchronizes maximize state with an external player/tool control.
    pub fn maximized(mut self, maximized: bool) -> Self {
        self.maximized = Some(maximized);
        self
    }

    /// Material Symbols glyph shown before the title.
    pub fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Translated title-row button labels.
    pub fn labels(mut self, labels: &'a DialogLabels) -> Self {
        self.labels = Some(labels);
        self
    }

    /// Block the rest of the app behind a dimmed backdrop; Esc closes the topmost modal.
    pub fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    /// Clicking the backdrop of a modal dialog requests closing.
    pub fn close_on_backdrop(mut self, close: bool) -> Self {
        self.close_on_backdrop = close;
        self
    }

    /// Initial outer width.
    pub fn default_width(mut self, width: f32) -> Self {
        self.default_size.x = width;
        self
    }

    /// Initial outer height; zero uses the minimum height.
    pub fn default_height(mut self, height: f32) -> Self {
        self.default_size.y = height;
        self
    }

    /// Fit unresized height to content, with scrolling bounded by the viewport.
    pub fn fit_content_height(mut self, fit: bool) -> Self {
        self.fit_content_height = fit;
        self
    }

    /// Smallest outer size the user can resize to.
    pub fn min_size(mut self, size: Vec2) -> Self {
        self.min_size = size;
        self
    }

    /// Allow resizing from the edges and corners (default `true`).
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Wrap the content in a vertical scroll area (default `true`). Disable for content
    /// that manages its own scrolling.
    pub fn scroll(mut self, scroll: bool) -> Self {
        self.scroll = scroll;
        self
    }

    /// Also scroll horizontally when content is wider than the dialog.
    pub fn hscroll(mut self, hscroll: bool) -> Self {
        self.hscroll = hscroll;
        self
    }

    /// Disable all interaction, e.g. while a child dialog is open.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The area dialogs are constrained to.
    pub fn bounds(ctx: &Context) -> Rect {
        let content = ctx.content_rect();
        let inset = DIALOG_INSET
            .min(content.width() / 4.0)
            .min(content.height() / 4.0)
            .max(0.0);
        content.shrink(inset)
    }

    /// Show the dialog and its content.
    pub fn show<R>(mut self, ctx: &Context, add: impl FnOnce(&mut Ui) -> R) -> DialogResponse<R> {
        let open = self.open.take();
        crate::fonts::ensure_icon_font(ctx);
        let default_labels = DialogLabels::default();
        let labels = self.labels.unwrap_or(&default_labels);
        let touch_allowed = crate::widgets::touch_activation_allowed(ctx);
        let bounds = Self::bounds(ctx);
        let mut state = DialogState::load(ctx, self.id);
        if let Some(maximized) = self.maximized
            && maximized != state.maximized
        {
            if maximized {
                state.restore = state.rect;
            } else {
                state.restore_pending = state.restore.is_some();
            }
            state.maximized = maximized;
        }
        let order = if self.modal {
            Order::Foreground
        } else {
            Order::Middle
        };
        let window_id = if state.hidden {
            self.id.with("hidden")
        } else {
            self.id
        };
        let layer = LayerId::new(order, window_id);
        let mut close = false;

        if self.modal {
            let backdrop = egui::Area::new(self.id.with("backdrop"))
                .order(Order::Foreground)
                .fixed_pos(ctx.content_rect().min)
                .interactable(true)
                .show(ctx, |ui| {
                    let rect = ui.ctx().content_rect();
                    let response = ui.allocate_rect(rect, Sense::CLICK | Sense::DRAG);
                    let alpha = if Palette::of(ui.ctx()).dark { 120 } else { 90 };
                    ui.painter()
                        .rect_filled(rect, 0.0, Color32::from_black_alpha(alpha));
                    response
                })
                .inner;
            if backdrop.clicked()
                && self.close_on_backdrop
                && touch_allowed
                && !egui::Popup::is_any_open(ctx)
            {
                close = true;
            }
            let is_top = ctx.memory_mut(|memory| {
                memory.set_modal_layer(layer);
                memory.top_modal_layer() == Some(layer)
            });
            if is_top
                && self.enabled
                && !egui::Popup::is_any_open(ctx)
                && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape))
            {
                close = true;
            }
        }

        let frame = Frame::window(&ctx.global_style());
        let mut window = egui::Window::new(self.title)
            .id(window_id)
            .title_bar(false)
            .collapsible(false)
            .order(order)
            .frame(frame)
            .enabled(self.enabled)
            .constrain_to(bounds)
            .max_size(bounds.size());
        let default_height = if self.default_size.y > 0.0 {
            self.default_size.y
        } else {
            self.min_size.y
        };
        let default_size = vec2(self.default_size.x, default_height)
            .min(bounds.size())
            .max(Vec2::ZERO);
        if state.hidden {
            let pos = state.rect.map_or(bounds.min, |rect| rect.min);
            window = window.auto_sized().current_pos(pos);
        } else if state.maximized {
            window = window.fixed_rect(bounds);
        } else {
            window = window
                .min_size(self.min_size.min(bounds.size()))
                .default_size(default_size)
                .default_pos(bounds.center() - default_size / 2.0)
                .resizable(self.resizable);
            if state.restore_pending
                && let Some(rect) = state.restore
            {
                window = window
                    .current_pos(rect.min)
                    .fixed_size(rect.size().min(bounds.size()));
                state.restore_pending = false;
                // egui constrains the new position with the area's last (maximized)
                // size, which would pin the restored window to the corner.
                if let Some(mut area) = egui::AreaState::load(ctx, window_id) {
                    area.size = Some(rect.size().min(bounds.size()));
                    area.set_left_top_pos(rect.min);
                    ctx.memory_mut(|memory| memory.areas_mut().set_state(layer, area));
                }
            }
        }

        state.user_sized |= !state.hidden && !state.maximized && Self::edge_dragged(ctx, layer);
        let before = state;
        let mut toggle_hidden = false;
        let mut toggle_maximized = false;
        let fit_height = bounds.height();
        let shown = window.show(ctx, |ui| {
            let title = self.title_row(ui, labels, &before, touch_allowed);
            close |= title.close;
            toggle_hidden = title.toggle_hidden;
            toggle_maximized = title.toggle_maximized;
            if before.hidden {
                return None;
            }
            ui.separator();
            if !self.scroll {
                return Some(add(ui));
            }
            let fit = !before.maximized && !before.user_sized;
            let hbar = if self.hscroll {
                ui.spacing().scroll.allocated_width()
            } else {
                0.0
            };
            let chrome = TITLE_ROW_HEIGHT
                + ui.spacing().item_spacing.y * 3.0
                + frame.total_margin().sum().y
                + hbar
                + 6.0;
            // Very short windows may leave little room; never overflow the app window.
            let max_height = if fit {
                (fit_height - chrome).max(0.0)
            } else {
                ui.available_height().max(0.0)
            };
            let area = if self.hscroll {
                egui::ScrollArea::both()
            } else {
                egui::ScrollArea::vertical()
            };
            Some(
                ui.vertical(|ui| {
                    if fit && self.fit_content_height {
                        // Changing bounds resets egui's cursor; isolate it from the caption.
                        ui.set_max_height(max_height);
                    }
                    area.id_salt(self.id.with("body"))
                        .max_height(max_height)
                        .min_scrolled_height(if fit && !self.fit_content_height {
                            max_height
                        } else {
                            max_height.min(64.0)
                        })
                        .auto_shrink([false, fit])
                        .show(ui, add)
                        .inner
                })
                .inner,
            )
        });

        let rect = shown.as_ref().map(|shown| shown.response.rect);
        if let Some(rect) = rect {
            if !state.maximized {
                state.rect = Some(rect);
            }
            if self.modal && !egui::Popup::is_any_open(ctx) {
                ctx.move_to_top(layer);
            }
        }
        if toggle_hidden {
            state.hidden = !state.hidden;
        }
        if toggle_maximized && !state.hidden {
            if state.maximized {
                state.maximized = false;
                state.restore_pending = state.restore.is_some();
            } else {
                state.restore = state.rect;
                state.maximized = true;
            }
        }
        let maximized = state.maximized;
        state.store(ctx, self.id);
        if close && let Some(open) = open {
            *open = false;
        }
        DialogResponse {
            inner: shown.and_then(|shown| shown.inner.flatten()),
            close_requested: close,
            rect,
            layer,
            maximized,
        }
    }

    /// Is the user dragging one of the window's resize edges? (egui's edge ids.)
    fn edge_dragged(ctx: &Context, layer: LayerId) -> bool {
        let id = Id::unique(layer).with("edge_drag");
        [
            "right",
            "left",
            "bottom",
            "top",
            "right_bottom",
            "right_top",
            "left_bottom",
            "left_top",
        ]
        .into_iter()
        .any(|edge| {
            ctx.read_response(id.with(edge))
                .is_some_and(|response| response.dragged())
        })
    }

    fn title_row(
        &self,
        ui: &mut Ui,
        labels: &DialogLabels,
        state: &DialogState,
        touch_allowed: bool,
    ) -> TitleActions {
        let p = Palette::of_ui(ui);
        let mut actions = TitleActions::default();
        // Registered before the caption buttons so they stay on top for hit testing.
        let title_click = ui.interact(
            Rect::from_min_size(
                ui.cursor().min,
                vec2(ui.available_width(), TITLE_ROW_HEIGHT),
            ),
            self.id.with("title_click"),
            Sense::CLICK,
        );
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), TITLE_ROW_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                let caption_width = if state.hidden {
                    2.0 * Tokens::ICON_BUTTON + Tokens::SPACE_XS
                } else {
                    3.0 * Tokens::ICON_BUTTON + 2.0 * Tokens::SPACE_XS
                };
                if let Some(icon) = self.icon {
                    ui.label(
                        RichText::new(icon)
                            .font(FontId::proportional(crate::icons::SIZE_SMALL))
                            .color(p.accent),
                    );
                }
                ui.add_sized(
                    vec2(
                        (ui.available_width() - caption_width - ui.spacing().item_spacing.x)
                            .max(0.0),
                        TITLE_ROW_HEIGHT,
                    ),
                    egui::Label::new(
                        RichText::new(self.title)
                            .strong()
                            .size(Tokens::FONT_HEADING)
                            .color(p.text),
                    )
                    .truncate()
                    .selectable(false),
                );
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = Tokens::SPACE_XS;
                    ui.add_space((ui.available_width() - caption_width).max(0.0));
                    let (icon, label) = if state.hidden {
                        (crate::icons::EXPAND, &labels.show)
                    } else {
                        (crate::icons::MINIMIZE, &labels.hide)
                    };
                    actions.toggle_hidden =
                        chrome_button(ui, icon, label, false).clicked() && touch_allowed;
                    if !state.hidden {
                        let (icon, label) = if state.maximized {
                            (crate::icons::RESTORE, &labels.restore)
                        } else {
                            (crate::icons::MAXIMIZE, &labels.maximize)
                        };
                        actions.toggle_maximized =
                            chrome_button(ui, icon, label, false).clicked() && touch_allowed;
                    }
                    actions.close = chrome_button(ui, crate::icons::CLOSE, &labels.close, true)
                        .clicked()
                        && touch_allowed;
                });
            },
        );
        if title_click.double_clicked() && touch_allowed {
            if state.hidden {
                actions.toggle_hidden = true;
            } else {
                actions.toggle_maximized = true;
            }
        }
        actions
    }
}

#[derive(Default)]
struct TitleActions {
    close: bool,
    toggle_hidden: bool,
    toggle_maximized: bool,
}

/// A 28 px title-row button with a 16 px glyph (Fluent caption-button sizing).
fn chrome_button(ui: &mut Ui, icon: &str, label: &str, destructive: bool) -> Response {
    let p = Palette::of_ui(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(Tokens::ICON_BUTTON), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), label));
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() && ui.is_enabled();
        let fill = match (hovered, destructive) {
            (true, true) => p.danger,
            (true, false) => p.surface_alt,
            _ => Color32::TRANSPARENT,
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(Tokens::RADIUS_S), fill);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(Tokens::RADIUS_S),
                Stroke::new(2.0, p.accent),
                StrokeKind::Inside,
            );
        }
        let color = if !ui.is_enabled() {
            p.text_muted.gamma_multiply(0.5)
        } else if hovered && destructive {
            Color32::WHITE
        } else {
            p.text_muted
        };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(crate::icons::SIZE_SMALL),
            color,
        );
    }
    response.on_hover_text(label)
}

/// A wrapping row of tabs, each with a 16 px icon and a label, for multi-page dialogs.
/// Returns `true` when the selection changed.
pub fn tab_bar<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, tabs: &[(T, &str, &str)]) -> bool {
    crate::fonts::ensure_icon_font(ui.ctx());
    let p = Palette::of_ui(ui);
    let touch_allowed = crate::widgets::touch_activation_allowed(ui.ctx());
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = Tokens::SPACE_XS;
        for (tab, icon, label) in tabs {
            let selected = *value == *tab;
            let font = FontId::proportional(Tokens::FONT_BODY);
            let label_width = ui
                .painter()
                .layout_no_wrap((*label).to_owned(), font.clone(), p.text)
                .size()
                .x;
            let size = vec2(
                crate::icons::SIZE_SMALL + Tokens::SPACE_S + label_width + Tokens::SPACE_M * 2.0,
                Tokens::ICON_BUTTON + 4.0,
            );
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            response
                .widget_info(|| WidgetInfo::selected(Role::Tab, ui.is_enabled(), selected, *label));
            if response.clicked() && touch_allowed && ui.is_enabled() && !selected {
                *value = *tab;
                changed = true;
            }
            if ui.is_rect_visible(rect) {
                let hovered = response.hovered() && ui.is_enabled();
                if selected || hovered {
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::same(Tokens::RADIUS_S),
                        if selected {
                            p.accent_soft
                        } else {
                            p.surface_alt
                        },
                    );
                }
                if selected {
                    let bar = Rect::from_min_max(
                        Pos2::new(rect.left() + 6.0, rect.bottom() - 2.5),
                        Pos2::new(rect.right() - 6.0, rect.bottom()),
                    );
                    ui.painter()
                        .rect_filled(bar, CornerRadius::same(1), p.accent);
                }
                if response.has_focus() {
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(Tokens::RADIUS_S),
                        Stroke::new(2.0, p.accent),
                        StrokeKind::Inside,
                    );
                }
                let icon_color = if selected { p.accent } else { p.text_muted };
                let left = rect.left() + Tokens::SPACE_M;
                ui.painter().text(
                    Pos2::new(left, rect.center().y),
                    Align2::LEFT_CENTER,
                    *icon,
                    FontId::proportional(crate::icons::SIZE_SMALL),
                    icon_color,
                );
                ui.painter().text(
                    Pos2::new(
                        left + crate::icons::SIZE_SMALL + Tokens::SPACE_S,
                        rect.center().y,
                    ),
                    Align2::LEFT_CENTER,
                    *label,
                    font,
                    if selected { p.text } else { p.text_muted },
                );
            }
            response.on_hover_text(*label);
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ctx: &Context, input: egui::RawInput, mut add: impl FnMut(&Context)) {
        let mut output = ctx.run_ui(input, |ui| add(ui.ctx()));
        output.textures_delta.clear();
    }

    fn input(size: Vec2) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        }
    }

    fn show(ctx: &Context, modal: bool) -> DialogResponse<()> {
        Dialog::new(Id::unique("test_dialog"), "Test")
            .modal(modal)
            .default_width(300.0)
            .show(ctx, |ui| {
                for line in 0..80 {
                    ui.label(format!("Line {line}"));
                }
            })
    }

    #[test]
    fn long_content_fits_inside_the_app_window() {
        let ctx = Context::default();
        let mut last = None;
        for _ in 0..4 {
            run(&ctx, input(vec2(800.0, 500.0)), |ctx| {
                last = show(ctx, false).rect
            });
        }
        let rect = last.expect("dialog rect");
        let bounds = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 500.0)).shrink(DIALOG_INSET);
        assert!(
            bounds.expand(1.0).contains_rect(rect),
            "{rect:?} outside {bounds:?}"
        );
        assert!(
            rect.height() > 300.0,
            "long content should use the available height: {rect:?}"
        );
    }

    #[test]
    fn short_confirmations_fit_content_instead_of_filling_available_height() {
        let ctx = Context::default();
        let id = Id::unique("compact-confirmation");
        for _ in 0..4 {
            run(&ctx, input(vec2(1280.0, 900.0)), |ctx| {
                confirm_dialog(
                    ctx,
                    "compact-confirmation",
                    "Save changes?",
                    "Do you want to save your changes before exiting?",
                    &[
                        ("Save", DialogButton::Primary),
                        ("Don't Save", DialogButton::Danger),
                        ("Cancel", DialogButton::Secondary),
                    ],
                );
            });
        }
        let rect = ctx.memory(|memory| memory.area_rect(id)).unwrap();
        assert!(rect.width() <= 460.0, "{rect:?}");
        assert!(rect.height() <= 220.0, "{rect:?}");
    }

    #[test]
    fn content_sized_dialog_still_scrolls_long_content_within_the_viewport() {
        let ctx = Context::default();
        let mut last = None;
        for _ in 0..4 {
            run(&ctx, input(vec2(800.0, 500.0)), |ctx| {
                last = Dialog::new(Id::unique("content-sized"), "Long confirmation")
                    .fit_content_height(true)
                    .show(ctx, |ui| {
                        for line in 0..80 {
                            ui.label(format!("Line {line}"));
                        }
                    })
                    .rect;
            });
        }
        let rect = last.unwrap();
        assert!(
            Dialog::bounds(&ctx).expand(1.0).contains_rect(rect),
            "{rect:?}"
        );
        assert!(rect.height() > 300.0, "{rect:?}");
    }

    #[test]
    fn content_sized_body_does_not_overlap_the_caption() {
        let ctx = Context::default();
        let mut last = None;
        let mut body = None;
        for _ in 0..4 {
            run(&ctx, input(vec2(800.0, 500.0)), |ctx| {
                last = Dialog::new(Id::unique("caption-body"), "Caption")
                    .fit_content_height(true)
                    .show(ctx, |ui| {
                        body = Some(ui.button("First body control").rect);
                    })
                    .rect;
            });
        }
        let rect = last.unwrap();
        let body = body.unwrap();
        assert!(
            body.top() > rect.top() + TITLE_ROW_HEIGHT,
            "{body:?} vs {rect:?}"
        );
        assert!(rect.contains_rect(body));
    }

    #[test]
    fn external_player_controls_maximize_and_restore_the_shared_frame() {
        let ctx = Context::default();
        let mut last = None;
        let mut original = None;
        for requested in [false, true, false] {
            for _ in 0..4 {
                run(&ctx, input(vec2(900.0, 600.0)), |ctx| {
                    let response = Dialog::new(Id::unique("controlled-dialog"), "Player")
                        .default_width(400.0)
                        .maximized(requested)
                        .show(ctx, |ui| {
                            ui.label("Slide");
                        });
                    assert_eq!(response.maximized, requested);
                    last = response.rect;
                });
            }
            let rect = last.unwrap();
            if requested {
                assert!((rect.width() - Dialog::bounds(&ctx).width()).abs() < 2.0);
            } else if let Some(before) = original {
                assert!((rect.size() - before).length() < 2.0);
            } else {
                original = Some(rect.size());
            }
        }
    }

    #[test]
    fn maximize_fills_the_bounds_and_restore_returns_to_the_previous_rect() {
        let ctx = Context::default();
        let id = Id::unique("test_dialog");
        let mut last = None;
        for _ in 0..3 {
            run(&ctx, input(vec2(900.0, 600.0)), |ctx| {
                last = show(ctx, false).rect
            });
        }
        let before = last.expect("rect");
        let mut state = DialogState::load(&ctx, id);
        state.restore = state.rect;
        state.maximized = true;
        state.store(&ctx, id);
        for _ in 0..2 {
            run(&ctx, input(vec2(900.0, 600.0)), |ctx| {
                last = show(ctx, false).rect
            });
        }
        let maximized = last.expect("rect");
        let bounds = Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0)).shrink(DIALOG_INSET);
        assert!(
            (maximized.width() - bounds.width()).abs() < 2.0,
            "{maximized:?}"
        );
        assert!(
            (maximized.height() - bounds.height()).abs() < 2.0,
            "{maximized:?}"
        );
        let mut state = DialogState::load(&ctx, id);
        state.maximized = false;
        state.restore_pending = true;
        state.store(&ctx, id);
        for _ in 0..3 {
            run(&ctx, input(vec2(900.0, 600.0)), |ctx| {
                last = show(ctx, false).rect
            });
        }
        let restored = last.expect("rect");
        assert!(
            (restored.min - before.min).length() < 2.0,
            "{restored:?} vs {before:?}"
        );
        assert!(
            (restored.size() - before.size()).length() < 2.0,
            "{restored:?} vs {before:?}"
        );
    }

    #[test]
    fn hidden_dialog_shows_only_its_title_row() {
        let ctx = Context::default();
        let id = Id::unique("test_dialog");
        run(&ctx, input(vec2(800.0, 600.0)), |ctx| {
            let _ = show(ctx, false);
        });
        let mut state = DialogState::load(&ctx, id);
        state.hidden = true;
        state.store(&ctx, id);
        let mut response = None;
        for _ in 0..3 {
            run(&ctx, input(vec2(800.0, 600.0)), |ctx| {
                response = Some(show(ctx, false))
            });
        }
        let response = response.expect("response");
        assert!(response.inner.is_none());
        let rect = response.rect.expect("rect");
        assert!(rect.height() < TITLE_ROW_HEIGHT * 2.5, "{rect:?}");
    }

    #[test]
    fn escape_closes_a_modal_dialog() {
        let ctx = Context::default();
        for _ in 0..2 {
            run(&ctx, input(vec2(800.0, 600.0)), |ctx| {
                let _ = show(ctx, true);
            });
        }
        let mut escape = input(vec2(800.0, 600.0));
        escape.events.push(egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let mut closed = false;
        run(&ctx, escape, |ctx| closed = show(ctx, true).close_requested);
        assert!(closed);
    }
}
