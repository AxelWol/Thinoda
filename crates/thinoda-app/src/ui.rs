//! All app chrome: top bar, format panel, sheet tabs, status bar, outliner, home screen, dialogs.

use egui::{
    Align, Color32, Context, CornerRadius, Key, Layout, Margin, Modifiers, RichText, Sense,
    TextEdit, Ui, vec2,
};
use thinoda_canvas::{Action, BoardTool, MapCanvas, MapView, ObjectSelection};
use thinoda_core::numbering::{NumberFormat, Numbering};
use thinoda_core::{
    CardKind, Color, LineShape, Structure, TextAlign, TopicId, TopicShape, TopicStyle, markers,
    templates, theme::Theme,
};
use thinoda_platform::settings::{Language, ThemeMode as SettingsTheme};
use thinoda_ui::{
    Dialog, DialogButton, Palette, Tokens, confirm_dialog_with_labels as confirm_dialog, icons,
    widgets,
};

use crate::{
    app::{
        CenterMode, CloseIntent, Cmd, EXPORT_MENU, FocusField, FormatTab, SettingsTab,
        SidebarFamily, ThinodaApp,
    },
    context_menus::CtxEntry,
    i18n::{self, t},
    quick_styles::{StyleColor, StyleTargets},
};

type StyleChange = (&'static str, Box<dyn Fn(&mut TopicStyle)>);
type StyleToggle = fn(&mut TopicStyle, bool);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CardTextTarget {
    document: String,
    instance: u64,
    sheet: usize,
    root: TopicId,
    topic: TopicId,
    revision: u64,
}

#[derive(Clone, Copy)]
struct CardTitleRestore {
    instance: u64,
    sheet: usize,
    root: TopicId,
    topic: TopicId,
    serial: u64,
    frame: u64,
}

#[derive(Clone)]
struct CardTextFocusAfterEditor {
    target: CardTextTarget,
    serial: u64,
    control: egui::Id,
    frame: u64,
}

#[derive(Clone)]
struct DeferredCardTextCommand {
    instance: u64,
    sheet: usize,
    root: TopicId,
    command: CapturedCardTextCommand,
}

#[derive(Clone)]
enum CapturedCardTextCommand {
    Command(Cmd),
    CloseTab(Option<(u64, egui::ViewportId)>),
}

#[derive(Clone)]
struct DeferredCardTextOperation {
    origin: egui::ViewportId,
    not_before_root_frame: u64,
    action: DeferredCardTextAction,
}

// A command is already ordered in the current input batch; an asynchronous
// result must wait for input queued before it arrived.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CardOpenOrder {
    CurrentInput,
    AfterQueuedInput,
}

#[derive(Clone)]
enum DeferredCardTextAction {
    Open {
        path: std::path::PathBuf,
        target_instance: Option<u64>,
        order: CardOpenOrder,
    },
    SaveAll,
    Command {
        instance: u64,
        sheet: usize,
        root: TopicId,
        command: CapturedCardTextCommand,
    },
}

#[derive(Clone, Default)]
struct DeferredCardTextInput {
    events: Vec<egui::Event>,
    close_after_frame: Option<u64>,
}

pub(crate) struct CardTextInputPlugin;

impl egui::Plugin for CardTextInputPlugin {
    fn debug_name(&self) -> &'static str {
        "card text input ordering"
    }

    fn input_hook(&mut self, ctx: &Context, input: &mut egui::RawInput) {
        prepare_card_text_input(ctx, input);
    }
}

enum CardTextChange {
    Bold(bool),
    Italic(bool),
    Underline(bool),
    Strikethrough(bool),
    Align(TextAlign),
    Family(Option<String>),
    Size(f32),
    Color(Option<Color>),
}

impl CardTextChange {
    fn label(&self) -> &'static str {
        match self {
            Self::Bold(_) | Self::Italic(_) | Self::Underline(_) | Self::Strikethrough(_) => {
                "Text Style"
            }
            Self::Align(_) => "Align",
            Self::Family(_) => "Font Family",
            Self::Size(_) => "Font Size",
            Self::Color(_) => "Text Color",
        }
    }

    fn apply(&self, style: &mut TopicStyle) {
        match self {
            Self::Bold(value) => style.bold = Some(*value),
            Self::Italic(value) => style.italic = Some(*value),
            Self::Underline(value) => style.underline = Some(*value),
            Self::Strikethrough(value) => style.strikethrough = Some(*value),
            Self::Align(value) => style.align = Some(*value),
            Self::Family(value) => style.font_family = value.clone(),
            Self::Size(value) => style.font_size = Some(*value),
            Self::Color(value) => style.text_color = *value,
        }
    }
}

fn card_text_slot(viewport: egui::ViewportId) -> egui::Id {
    egui::Id::unique(("thinoda_card_text_target", viewport))
}

fn card_text_input_is_action(event: &egui::Event) -> bool {
    !matches!(
        event,
        egui::Event::ModifiersChanged(_)
            | egui::Event::PointerMoved(_)
            | egui::Event::Key { pressed: false, .. }
            | egui::Event::Key {
                key: Key::AltLeft
                    | Key::AltRight
                    | Key::ControlLeft
                    | Key::ControlRight
                    | Key::ShiftLeft
                    | Key::ShiftRight
                    | Key::SuperLeft
                    | Key::SuperRight,
                ..
            }
    )
}

fn card_text_input_is_keyboard_action(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Text(_)
            | egui::Event::Paste(_)
            | egui::Event::Cut
            | egui::Event::Copy
            | egui::Event::Ime(_)
            | egui::Event::Key { pressed: true, .. }
    ) && card_text_input_is_action(event)
}

fn card_text_pointer_packet(events: &[egui::Event], index: usize) -> (usize, usize) {
    let companion = |event: &egui::Event| {
        matches!(
            event,
            egui::Event::Touch { .. }
                | egui::Event::PointerMoved(_)
                | egui::Event::PointerGone
                | egui::Event::MouseMoved(_)
        )
    };
    let mut start = index;
    while start > 0 && companion(&events[start - 1]) {
        start -= 1;
    }
    let mut end = index + 1;
    while end < events.len() && companion(&events[end]) {
        end += 1;
    }
    (start, end)
}

fn card_text_shortcut_start(events: &[egui::Event], index: usize, modifiers: Modifiers) -> usize {
    let mut start = index;
    while start > 0 {
        let part_of_chord = match &events[start - 1] {
            egui::Event::ModifiersChanged(current) => current.matches_exact(modifiers),
            egui::Event::Key {
                key, pressed: true, ..
            } => match key {
                Key::AltLeft | Key::AltRight => modifiers.alt,
                Key::ControlLeft | Key::ControlRight => modifiers.ctrl,
                Key::ShiftLeft | Key::ShiftRight => modifiers.shift,
                Key::SuperLeft | Key::SuperRight => modifiers.mac_cmd || modifiers.command,
                _ => false,
            },
            _ => false,
        };
        if !part_of_chord {
            break;
        }
        start -= 1;
    }
    start
}

fn card_text_input_split(events: &[egui::Event]) -> Option<usize> {
    for (index, event) in events.iter().enumerate() {
        let formatting_shortcut = matches!(
            event,
            egui::Event::Key {
                key: Key::F10,
                pressed: true,
                modifiers,
                ..
            } if modifiers.matches_logically(Modifiers::ALT)
        );
        let command_shortcut = matches!(
            event,
            egui::Event::Key {
                pressed: true,
                modifiers,
                ..
            } if (modifiers.command || modifiers.ctrl || modifiers.mac_cmd)
                && card_text_input_is_action(event)
        );
        let keyboard_transition = formatting_shortcut
            || command_shortcut
            || matches!(
                event,
                egui::Event::Key {
                    key: Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.matches_exact(Modifiers::NONE)
                    || modifiers.matches_exact(Modifiers::SHIFT)
            )
            || matches!(
                event,
                egui::Event::Key {
                    key: Key::Enter | Key::Escape,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.matches_exact(Modifiers::NONE)
            );
        let pointer_transition = matches!(event, egui::Event::PointerButton { .. });
        if keyboard_transition {
            if events[..index].iter().any(card_text_input_is_action) {
                let start = if let egui::Event::Key { modifiers, .. } = event
                    && (formatting_shortcut || command_shortcut)
                {
                    card_text_shortcut_start(events, index, *modifiers)
                } else {
                    index
                };
                return Some(start);
            }
            if events[index + 1..].iter().any(card_text_input_is_action) {
                return Some(index + 1);
            }
        } else if pointer_transition {
            let (start, end) = card_text_pointer_packet(events, index);
            if events[..start]
                .iter()
                .any(card_text_input_is_keyboard_action)
            {
                return Some(start);
            }
            if events[end..].iter().any(card_text_input_is_keyboard_action) {
                return Some(end);
            }
        }
    }
    None
}

fn card_text_input_needs_repaint(ctx: &Context, viewport: egui::ViewportId) -> bool {
    ctx.data(|data| {
        data.get_temp::<DeferredCardTextInput>(card_text_slot(viewport).with("deferred_input"))
            .is_some()
    })
}

fn card_text_input_in_scope(ctx: &Context, viewport: egui::ViewportId) -> bool {
    ctx.data(|data| {
        data.get_temp::<bool>(card_text_slot(viewport).with("input_scope"))
            .unwrap_or(false)
    })
}

pub(crate) fn card_text_input_has_pending_events(
    ctx: &Context,
    viewport: egui::ViewportId,
) -> bool {
    ctx.data(|data| {
        data.get_temp::<DeferredCardTextInput>(card_text_slot(viewport).with("deferred_input"))
            .is_some_and(|input| !input.events.is_empty())
    })
}

fn request_card_text_input_repaint(ctx: &Context, viewport: egui::ViewportId) {
    ctx.request_repaint_of(viewport);
    #[cfg(not(target_arch = "wasm32"))]
    {
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        ctx.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::RequestPaintWhileHidden,
        );
    }
}

pub(crate) fn take_synthetic_card_close(ctx: &Context) -> bool {
    let slot = card_text_slot(egui::ViewportId::ROOT).with("synthetic_close");
    ctx.data_mut(|data| {
        let synthetic = data.get_temp::<bool>(slot).unwrap_or(false);
        data.remove::<bool>(slot);
        synthetic
    })
}

pub(crate) fn authorize_synthetic_card_close(ctx: &Context) {
    ctx.data_mut(|data| {
        data.insert_temp(
            card_text_slot(egui::ViewportId::ROOT).with("authorized_close"),
            true,
        );
    });
}

fn prepare_card_text_input(ctx: &Context, input: &mut egui::RawInput) {
    let viewport = input.viewport_id;
    let slot = card_text_slot(viewport);
    if ctx.viewport_for(viewport, |state| state.output.num_completed_passes) == 0 {
        ctx.data_mut(|data| data.remove::<u64>(slot.with("after_editor")));
    }
    let eligible = card_text_input_in_scope(ctx, viewport);
    let pending =
        ctx.data(|data| data.get_temp::<DeferredCardTextInput>(slot.with("deferred_input")));
    if !eligible && pending.is_none() {
        return;
    }

    if ctx.viewport_for(viewport, |state| state.output.num_completed_passes) > 0 {
        return;
    }
    let frame = ctx.cumulative_frame_nr_for(viewport);
    let mut deferred = pending.unwrap_or_default();
    ctx.data_mut(|data| data.remove::<DeferredCardTextInput>(slot.with("deferred_input")));
    if !deferred.events.is_empty() {
        deferred.events.append(&mut input.events);
        input.events = std::mem::take(&mut deferred.events);
    }

    if eligible && let Some(split) = card_text_input_split(&input.events) {
        deferred.events = input.events.split_off(split);
    }

    let has_actions = input.events.iter().any(card_text_input_is_action);
    let close = input.viewport().close_requested();
    if close
        && (eligible || !deferred.events.is_empty() || deferred.close_after_frame.is_some())
        && (has_actions || !deferred.events.is_empty())
    {
        deferred.close_after_frame.get_or_insert(frame + 1);
        if let Some(viewport) = input.viewports.get_mut(&viewport) {
            viewport
                .events
                .retain(|event| *event != egui::ViewportEvent::Close);
        }
    }
    let child_has_pending_input = viewport == egui::ViewportId::ROOT
        && ctx.data(|data| {
            data.get_temp::<Vec<egui::ViewportId>>(slot.with("document_viewports"))
                .unwrap_or_default()
                .iter()
                .any(|child| {
                    data.get_temp::<DeferredCardTextInput>(
                        card_text_slot(*child).with("deferred_input"),
                    )
                    .is_some()
                })
        });
    let operations_pending = ctx.data(|data| {
        data.get_temp::<Vec<DeferredCardTextOperation>>(
            card_text_slot(egui::ViewportId::ROOT).with("deferred_operations"),
        )
        .is_some_and(|operations| {
            operations
                .iter()
                .any(|operation| viewport == egui::ViewportId::ROOT || operation.origin == viewport)
        })
    });
    if deferred
        .close_after_frame
        .is_some_and(|earliest| frame >= earliest)
        && !has_actions
        && deferred.events.is_empty()
        && !child_has_pending_input
        && !operations_pending
    {
        if let Some(viewport) = input.viewports.get_mut(&viewport)
            && !viewport.close_requested()
        {
            viewport.events.push(egui::ViewportEvent::Close);
        }
        deferred.close_after_frame = None;
        if viewport == egui::ViewportId::ROOT {
            ctx.data_mut(|data| data.insert_temp(slot.with("synthetic_close"), true));
        }
    }
    if !deferred.events.is_empty() || deferred.close_after_frame.is_some() {
        ctx.data_mut(|data| data.insert_temp(slot.with("deferred_input"), deferred));
        request_card_text_input_repaint(ctx, viewport);
    }
}

fn card_text_label() -> &'static str {
    t("Format whole card text")
}

fn card_text_icon(ui: &mut Ui, icon: &str, label: &str, selected: bool, focus: bool) -> bool {
    card_text_icon_response(ui, icon, label, selected, focus).0
}

fn card_text_owned_icon(
    ui: &mut Ui,
    icon: &str,
    label: &str,
    selected: bool,
    focus: bool,
    owned: &mut Vec<egui::Id>,
) -> bool {
    let (activated, id) = card_text_icon_response(ui, icon, label, selected, focus);
    owned.push(id);
    activated
}

fn card_text_icon_response(
    ui: &mut Ui,
    icon: &str,
    label: &str,
    selected: bool,
    focus: bool,
) -> (bool, egui::Id) {
    let response = widgets::icon_button(ui, icon, label, selected);
    if focus {
        response.request_focus();
    }
    if response.gained_focus() {
        response.scroll_to_me(None);
    }
    let keyboard = ui.is_enabled()
        && response.has_focus()
        && ui.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::Enter)
                || input.consume_key(Modifiers::NONE, Key::Space)
        });
    (
        ui.is_enabled()
            && ((widgets::touch_activation_allowed(ui.ctx()) && response.clicked()) || keyboard),
        response.id,
    )
}

fn card_text_swatch(ui: &mut Ui, color: Color, selected: bool, owned: &mut Vec<egui::Id>) -> bool {
    let value = from_color(color);
    let hex = if color.a == 255 {
        format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            color.r, color.g, color.b, color.a
        )
    };
    let label = format!("{} {hex}", t("Text Color"));
    let palette = Palette::of_ui(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    owned.push(response.id);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ColorWell, ui.is_enabled(), selected, &label)
    });
    if response.gained_focus() {
        response.scroll_to_me(None);
    }
    if ui.is_rect_visible(rect) {
        let swatch = egui::Rect::from_center_size(rect.center(), vec2(18.0, 18.0));
        ui.painter()
            .rect_filled(swatch.shrink(2.0), Tokens::RADIUS_S, value);
        ui.painter().rect_stroke(
            swatch,
            Tokens::RADIUS_S,
            egui::Stroke::new(
                if selected { 2.0 } else { 1.0 },
                if selected {
                    palette.accent
                } else {
                    palette.border
                },
            ),
            egui::StrokeKind::Inside,
        );
        if response.has_focus() && ui.is_enabled() {
            ui.painter().rect_stroke(
                rect,
                Tokens::RADIUS_S,
                egui::Stroke::new(2.0, palette.text),
                egui::StrokeKind::Inside,
            );
        }
    }
    let keyboard = ui.is_enabled()
        && response.has_focus()
        && ui.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::Enter)
                || input.consume_key(Modifiers::NONE, Key::Space)
        });
    let activated = ui.is_enabled()
        && ((widgets::touch_activation_allowed(ui.ctx()) && response.clicked()) || keyboard);
    response.on_hover_text(label);
    activated
}

fn fim_hierarchy_shortcut(action: &Action) -> bool {
    matches!(
        action,
        Action::MoveUp
            | Action::MoveDown
            | Action::Indent
            | Action::Outdent
            | Action::SortAscending
            | Action::SortDescending
    )
}

fn fim_canvas_owns_keyboard(ctx: &Context, view: &MapView) -> bool {
    let Some(focused) = ctx.memory(|memory| memory.focused()) else {
        return true;
    };
    let canvas = MapCanvas::id();
    if focused == canvas {
        return true;
    }

    // Board items and handles are separate focusable canvas children. Match
    // their IDs so FIM routing also applies when an accessible child owns
    // focus, while leaving unrelated controls and text editors alone.
    let mut items = view
        .selection
        .iter()
        .copied()
        .map(thinoda_core::SceneId::Topic)
        .chain(
            view.board_selection
                .iter()
                .copied()
                .map(thinoda_core::SceneId::Object),
        )
        .chain(match view.selected_object {
            Some(ObjectSelection::Zone(id)) => Some(thinoda_core::SceneId::Zone(id)),
            _ => None,
        });
    items.any(|item| {
        focused == canvas.with(("board-item", item))
            || (0..=3).any(|part| focused == canvas.with(("board-control", item, part)))
    })
}

fn data_divider(ui: &mut Ui) {
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}

/// An always-visible data group whose heading also labels its editor.
fn data_section<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui, egui::Id) -> R) -> R {
    data_divider(ui);
    let heading = ui.strong(title);
    ui.add_space(4.0);
    ui.vertical(|ui| add(ui, heading.id)).inner
}

#[derive(Clone, Copy)]
struct NumericDrag {
    owner: Option<egui::Id>,
    value: f32,
    frame: u64,
    canceled: bool,
}

pub(crate) fn touch_numeric_value(
    ui: &mut Ui,
    value: &mut f32,
    salt: &'static str,
    owner: Option<egui::Id>,
    add: impl FnOnce(&mut Ui, &mut f32) -> egui::Response,
) -> (egui::Response, bool) {
    let id = ui.make_persistent_id(("touch_slider_value", salt));
    let frame = ui.ctx().cumulative_frame_nr();
    let pending = ui.ctx().data(|data| data.get_temp::<NumericDrag>(id));
    let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
    let canceled = owner.is_none()
        || !ui.is_enabled()
        || !touch_allowed
        || pending.is_some_and(|draft| {
            draft.canceled || draft.owner != owner || frame > draft.frame.saturating_add(1)
        });
    if let Some(draft) = pending.filter(|_| !canceled) {
        *value = draft.value;
    }
    let response = add(ui, value);
    let pointer_down = ui.input(|input| input.pointer.primary_down());

    if canceled {
        ui.ctx().data_mut(|data| {
            if pointer_down {
                data.insert_temp(
                    id,
                    NumericDrag {
                        owner,
                        value: *value,
                        frame,
                        canceled: true,
                    },
                );
            } else {
                data.remove::<NumericDrag>(id);
            }
        });
        return (response, false);
    }
    if pointer_down {
        if response.changed() || pending.is_some() {
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    id,
                    NumericDrag {
                        owner,
                        value: *value,
                        frame,
                        canceled: false,
                    },
                );
            });
        }
        return (response, false);
    }
    if response.changed() || pending.is_some() {
        ui.ctx().data_mut(|data| data.remove::<NumericDrag>(id));
        return (response, true);
    }
    (response, false)
}

pub(crate) fn scroll_menu(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    let margin = egui::Frame::menu(ui.style()).total_margin().sum().y;
    let height = (ui.ctx().content_rect().height() - margin).max(0.0);
    // A popup's initial sizing height must not become the permanent scroll limit.
    ui.set_max_height(height);
    egui::ScrollArea::vertical()
        .id_salt("menu_scroll")
        .max_height(height)
        .min_scrolled_height(0.0)
        .show(ui, |ui| {
            content(ui);
            reveal_changed_focus(ui);
        });
}

pub(crate) fn reveal_changed_focus(ui: &mut Ui) {
    let focus = ui.memory(|memory| memory.focused());
    let focus_key = ui.scope_id().with("scroll_focus");
    let previous = ui.data_mut(|data| {
        let previous = data.get_temp::<Option<egui::Id>>(focus_key);
        data.insert_temp(focus_key, focus);
        previous
    });
    if previous != Some(focus)
        && let Some(focused) = focus
        && let Some(response) = ui.ctx().read_response(focused)
        && response.layer_id == ui.layer_id()
        && response_is_within(ui, &response)
    {
        ui.scroll_to_rect(response.rect, None);
    }
}

pub(crate) fn response_is_within(ui: &Ui, response: &egui::Response) -> bool {
    let mut current = response.clone();
    // Bound the walk in case conflicting widget IDs form a cycle.
    for _ in 0..1000 {
        let parent = current.parent_id();
        if parent == ui.unique_id() {
            return true;
        }
        if parent == current.id {
            return false;
        }
        let Some(response) = ui.ctx().read_response(parent) else {
            return false;
        };
        current = response;
    }
    false
}

#[derive(Clone, Copy)]
struct ToolbarSweepEvent {
    x: f32,
    start: usize,
    end: usize,
    delta: i32,
}

// Range-add/min tree over candidate y coordinates. A zero is an uncovered
// coordinate; range updates and nearest-zero queries are logarithmic.
struct ToolbarCoverage {
    len: usize,
    minimum: Vec<i32>,
    lazy: Vec<i32>,
}

impl ToolbarCoverage {
    fn new(len: usize) -> Self {
        Self {
            len,
            minimum: vec![0; len * 4],
            lazy: vec![0; len * 4],
        }
    }

    fn add(&mut self, start: usize, end: usize, delta: i32) {
        self.add_in(1, 0..self.len, start..end, delta);
    }

    fn add_in(
        &mut self,
        node: usize,
        span: std::ops::Range<usize>,
        update: std::ops::Range<usize>,
        delta: i32,
    ) {
        if update.start <= span.start && span.end <= update.end {
            self.minimum[node] += delta;
            self.lazy[node] += delta;
            return;
        }
        let middle = (span.start + span.end) / 2;
        if update.start < middle {
            self.add_in(node * 2, span.start..middle, update.clone(), delta);
        }
        if middle < update.end {
            self.add_in(node * 2 + 1, middle..span.end, update, delta);
        }
        self.minimum[node] =
            self.lazy[node] + self.minimum[node * 2].min(self.minimum[node * 2 + 1]);
    }

    fn first_clear(&self, start: usize) -> Option<usize> {
        self.first_clear_in(1, 0..self.len, start, 0)
    }

    fn first_clear_in(
        &self,
        node: usize,
        span: std::ops::Range<usize>,
        start: usize,
        inherited: i32,
    ) -> Option<usize> {
        if span.end <= start || inherited + self.minimum[node] > 0 {
            return None;
        }
        if span.end - span.start == 1 {
            return Some(span.start);
        }
        let middle = (span.start + span.end) / 2;
        let inherited = inherited + self.lazy[node];
        self.first_clear_in(node * 2, span.start..middle, start, inherited)
            .or_else(|| self.first_clear_in(node * 2 + 1, middle..span.end, start, inherited))
    }

    fn last_clear(&self, end: usize) -> Option<usize> {
        self.last_clear_in(1, 0..self.len, end, 0)
    }

    fn last_clear_in(
        &self,
        node: usize,
        span: std::ops::Range<usize>,
        end: usize,
        inherited: i32,
    ) -> Option<usize> {
        if end <= span.start || inherited + self.minimum[node] > 0 {
            return None;
        }
        if span.end - span.start == 1 {
            return Some(span.start);
        }
        let middle = (span.start + span.end) / 2;
        let inherited = inherited + self.lazy[node];
        self.last_clear_in(node * 2 + 1, middle..span.end, end, inherited)
            .or_else(|| self.last_clear_in(node * 2, span.start..middle, end, inherited))
    }
}

pub(crate) fn nearest_clear_toolbar_rect(
    desired: egui::Rect,
    available: egui::Rect,
    obstacles: &[egui::Rect],
    clearance: f32,
) -> Option<egui::Rect> {
    let size = desired.size();
    if !available.is_positive()
        || size.x <= 0.0
        || size.y <= 0.0
        || size.x > available.width()
        || size.y > available.height()
    {
        return None;
    }
    let min = available.min;
    let max = available.max - size;
    let target = egui::pos2(
        desired.left().clamp(min.x, max.x),
        desired.top().clamp(min.y, max.y),
    );
    let at_target = egui::Rect::from_min_size(target, size);
    if obstacles
        .iter()
        .all(|obstacle| !at_target.intersects(obstacle.expand(clearance)))
    {
        return Some(at_target);
    }

    // In toolbar-top-left coordinates, each obstacle expands by the toolbar
    // size. Points on the expanded boundary retain `clearance` from the real
    // obstacle, including after egui rounds the Area position to pixels.
    let domain = egui::Rect::from_min_max(min, max);
    let blocked: Vec<_> = obstacles
        .iter()
        .filter(|obstacle| obstacle.is_positive())
        .map(|obstacle| {
            egui::Rect::from_min_max(
                obstacle.min - size - vec2(clearance, clearance),
                obstacle.max + vec2(clearance, clearance),
            )
        })
        .filter(|rect| rect.intersects(domain))
        .collect();
    let mut xs = vec![min.x, max.x, target.x];
    let mut ys = vec![min.y, max.y, target.y];
    for rect in &blocked {
        for x in [rect.left(), rect.right()] {
            if min.x <= x && x <= max.x {
                xs.push(x);
            }
        }
        for y in [rect.top(), rect.bottom()] {
            if min.y <= y && y <= max.y {
                ys.push(y);
            }
        }
    }
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| *a == *b);
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| *a == *b);

    let mut events = Vec::with_capacity(blocked.len() * 2);
    for rect in blocked {
        // Edges of the expanded obstacles are safe, so only cover candidate
        // coordinates strictly inside each interval.
        let start = ys.partition_point(|y| *y <= rect.top());
        let end = ys.partition_point(|y| *y < rect.bottom());
        if start < end {
            events.push(ToolbarSweepEvent {
                x: rect.left(),
                start,
                end,
                delta: 1,
            });
            events.push(ToolbarSweepEvent {
                x: rect.right(),
                start,
                end,
                delta: -1,
            });
        }
    }
    events.sort_by(|a, b| a.x.total_cmp(&b.x));
    let target_y = ys.partition_point(|y| *y < target.y);
    let mut coverage = ToolbarCoverage::new(ys.len());
    let mut cursor = 0;
    let mut best: Option<(f32, egui::Rect)> = None;
    for x in xs {
        while cursor < events.len() && events[cursor].x < x {
            let event = events[cursor];
            coverage.add(event.start, event.end, event.delta);
            cursor += 1;
        }
        let first = cursor;
        while cursor < events.len() && events[cursor].x == x {
            cursor += 1;
        }
        for event in events[first..cursor].iter().filter(|event| event.delta < 0) {
            coverage.add(event.start, event.end, event.delta);
        }
        for index in [
            coverage.last_clear(target_y + 1),
            coverage.first_clear(target_y),
        ]
        .into_iter()
        .flatten()
        {
            let top_left = egui::pos2(x, ys[index]);
            let distance = (top_left - target).length_sq();
            if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                best = Some((distance, egui::Rect::from_min_size(top_left, size)));
            }
        }
        for event in events[first..cursor].iter().filter(|event| event.delta > 0) {
            coverage.add(event.start, event.end, event.delta);
        }
    }
    best.map(|(_, rect)| rect)
}

fn board_screen_rect(rect: thinoda_core::Rect, origin: egui::Pos2, zoom: f32) -> egui::Rect {
    let max = rect.max();
    egui::Rect::from_min_max(
        origin + vec2(rect.min.x, rect.min.y) * zoom,
        origin + vec2(max.x, max.y) * zoom,
    )
}

pub(crate) fn board_card_obstacles(
    sheet: &thinoda_core::Sheet,
    layout: &thinoda_core::Layout,
    origin: egui::Pos2,
    zoom: f32,
    canvas: egui::Rect,
) -> Vec<egui::Rect> {
    // Cards are floating roots. Build the first clipping frame for each root
    // once instead of looking through every frame for every card.
    let mut clips = std::collections::HashMap::new();
    for zone in sheet.zones.iter().filter(|zone| zone.clip) {
        for id in &zone.roots {
            clips.entry(*id).or_insert(zone.frame);
        }
    }
    layout
        .order
        .iter()
        .filter_map(|id| {
            let card = sheet.topic(*id)?.card?;
            let rect = layout.topics.get(id)?.rect;
            let rect = thinoda_core::board::rotated_bounds(rect, card.rotation);
            let mut visible = board_screen_rect(rect, origin, zoom);
            if let Some(clip) = clips.get(id) {
                visible = visible.intersect(board_screen_rect(*clip, origin, zoom));
            }
            visible = visible.intersect(canvas);
            visible.is_positive().then_some(visible)
        })
        .collect()
}

#[derive(Clone, PartialEq)]
struct TopicToolbarMeasureKey {
    document: String,
    ai_enabled: bool,
    has_link: bool,
    todo: bool,
    card_text: bool,
    language: i18n::Lang,
    available_size: [u32; 2],
    pixels_per_point: u32,
    style: std::sync::Arc<egui::Style>,
    font_key: u64,
}

fn topic_toolbar_measure_key(
    ctx: &Context,
    available: egui::Rect,
    document: &str,
    ai_enabled: bool,
    has_link: bool,
    todo: bool,
    card_text: bool,
) -> TopicToolbarMeasureKey {
    use std::hash::{Hash, Hasher};

    let font_key = ctx.fonts(|fonts| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let defs = fonts.definitions();
        defs.families.hash(&mut hasher);
        for (name, data) in &defs.font_data {
            name.hash(&mut hasher);
            std::sync::Arc::as_ptr(data).hash(&mut hasher);
        }
        for font in fonts.discovered_fonts() {
            font.name.hash(&mut hasher);
            font.data.index.hash(&mut hasher);
            (std::sync::Arc::as_ptr(&font.data.font) as *const ()).hash(&mut hasher);
            for family in &font.families {
                family.family.hash(&mut hasher);
                family.priority.hash(&mut hasher);
            }
        }
        hasher.finish()
    });
    TopicToolbarMeasureKey {
        document: document.to_owned(),
        ai_enabled,
        has_link,
        todo,
        card_text,
        language: i18n::lang(),
        available_size: [available.width().to_bits(), available.height().to_bits()],
        pixels_per_point: ctx.pixels_per_point().to_bits(),
        style: ctx.global_style(),
        font_key,
    }
}

fn menu_button(
    ui: &mut Ui,
    label: impl Into<egui::WidgetText>,
    content: impl FnOnce(&mut Ui),
) -> egui::InnerResponse<Option<()>> {
    ui.menu_button(label, |ui| scroll_menu(ui, content))
}

fn scroll_row(ui: &mut Ui, salt: &'static str, width: f32, content: impl FnOnce(&mut Ui)) {
    scroll_row_offset(ui, salt, width, ui.spacing().interact_size.y, None, content);
}

fn status_menu_button(
    ui: &mut Ui,
    label: impl Into<egui::WidgetText>,
    content: impl FnOnce(&mut Ui),
) -> egui::InnerResponse<Option<()>> {
    guarded_menu_button(ui, label, content)
}

pub(crate) fn guarded_menu_button(
    ui: &mut Ui,
    label: impl Into<egui::WidgetText>,
    content: impl FnOnce(&mut Ui),
) -> egui::InnerResponse<Option<()>> {
    let mut menu = egui::containers::menu::MenuButton::new(label.into());
    if !widgets::touch_activation_allowed(ui.ctx()) {
        menu.button = menu.button.sense(Sense::hover());
    }
    let (response, inner) = menu.ui(ui, |ui| scroll_menu(ui, content));
    egui::InnerResponse::new(inner.map(|inner| inner.inner), response)
}

fn scroll_row_offset(
    ui: &mut Ui,
    salt: &'static str,
    width: f32,
    row_height: f32,
    offset: Option<f32>,
    content: impl FnOnce(&mut Ui),
) -> egui::scroll_area::ScrollAreaOutput<()> {
    ui.allocate_ui_with_layout(
        vec2(width, row_height),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
            ui.style_mut().always_scroll_the_only_direction = true;
            let mut scroll = egui::ScrollArea::horizontal()
                .id_salt(salt)
                .scroll_source(egui::scroll_area::ScrollSource::ALL)
                .max_width(width)
                .min_scrolled_height(row_height)
                .max_height(row_height)
                .auto_shrink([false, false])
                .min_scrolled_width(0.0)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
            if let Some(offset) = offset {
                scroll = scroll.horizontal_scroll_offset(offset);
            }
            scroll.show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(Tokens::SPACE_XS);
                    content(ui);
                    ui.add_space(Tokens::SPACE_XS);
                });
            })
        },
    )
    .inner
}

fn sheet_scroll_row(ui: &mut Ui, width: f32, content: impl FnOnce(&mut Ui, f32)) {
    let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
    let state_id = ui.make_persistent_id("sheet_scroll_buttons");
    let (position, max_offset) =
        ui.data(|data| data.get_temp::<(f32, f32)>(state_id).unwrap_or_default());
    let button_width = ui.spacing().interact_size.y;
    let names_width = (width - 2.0 * (button_width + ui.spacing().item_spacing.x)).max(0.0);
    let step = (names_width * 0.8).max(1.0);
    let mut offset = None;
    for (glyph, label, enabled, target) in [
        (
            "<",
            t("Scroll sheets left"),
            position > 0.5,
            (position - step).max(0.0),
        ),
        (
            ">",
            t("Scroll sheets right"),
            position < max_offset - 0.5,
            (position + step).min(max_offset),
        ),
    ] {
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(glyph).min_size(vec2(button_width, button_width)),
            )
            .accessible_name(label)
            .on_hover_text(label)
            .clicked()
            && touch_allowed
        {
            offset = Some(target);
        }
    }
    let output = scroll_row_offset(
        ui,
        "sheet_tabs",
        names_width,
        ui.spacing().interact_size.y,
        offset,
        |ui| content(ui, names_width),
    );
    ui.data_mut(|data| {
        data.insert_temp(
            state_id,
            (
                output.state.clamped_offset().x,
                (output.content_size.x - output.inner_rect.width()).max(0.0),
            ),
        );
    });
}

fn document_tabs(
    ui: &mut Ui,
    width: f32,
    docs: &[crate::doc::OpenDoc],
    visible_indices: &[usize],
    viewport: egui::ViewportId,
    active: usize,
) -> (Option<usize>, bool, Option<usize>) {
    let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
    let state_id = ui.make_persistent_id(("document_scroll_buttons", viewport));
    let (position, max_offset, previous_active) = ui.data(|data| {
        data.get_temp::<(f32, f32, String)>(state_id)
            .unwrap_or_default()
    });
    let active_key = visible_indices
        .contains(&active)
        .then(|| docs.get(active))
        .flatten()
        .map_or_else(String::new, |doc| doc.key.clone());
    let active_changed = previous_active != active_key;
    let button_width = ui.spacing().interact_size.y;
    let arrow_slot = button_width + ui.spacing().item_spacing.x;
    let names_width = (width - 2.0 * arrow_slot).max(0.0);
    let step = (names_width * 0.8).max(1.0);
    let mut offset = None;
    let mut selected = None;
    let mut close_tab = None;
    let mut new_tab = false;
    for (glyph, label, enabled, target) in [
        (
            "<",
            t("Scroll documents left"),
            position > 0.5,
            (position - step).max(0.0),
        ),
        (
            ">",
            t("Scroll documents right"),
            position < max_offset - 0.5,
            (position + step).min(max_offset),
        ),
    ] {
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(glyph).min_size(vec2(button_width, button_width)),
            )
            .accessible_name(label)
            .on_hover_text(label)
            .clicked()
            && touch_allowed
        {
            offset = Some(target);
        }
    }
    let row_height = ui.spacing().interact_size.y.max(Tokens::ICON_BUTTON) + Tokens::SPACE_S;
    let output = scroll_row_offset(ui, "document_tabs", names_width, row_height, offset, |ui| {
        ui.horizontal(|ui| {
            for &index in visible_indices {
                let Some(doc) = docs.get(index) else {
                    continue;
                };
                let label = format!("{}{}", if doc.is_dirty() { "• " } else { "" }, doc.title());
                let (response, close) = ui
                    .push_id(&doc.key, |ui| {
                        let palette = Palette::of_ui(ui);
                        let font = egui::TextStyle::Button.resolve(ui.style());
                        let text_width = ui
                            .fonts_mut(|fonts| {
                                fonts
                                    .layout_no_wrap(label.clone(), font, palette.text)
                                    .size()
                                    .x
                            })
                            .min(
                                (names_width - 2.0 * Tokens::ICON_BUTTON - 20.0).clamp(28.0, 240.0),
                            );
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(text_width + Tokens::ICON_BUTTON + 12.0, Tokens::ICON_BUTTON),
                            Sense::hover(),
                        );
                        ui.painter().rect(
                            rect,
                            CornerRadius::same(Tokens::RADIUS_M),
                            if index == active {
                                palette.accent_soft
                            } else {
                                palette.surface_alt
                            },
                            egui::Stroke::new(
                                1.0,
                                if index == active {
                                    palette.accent
                                } else {
                                    palette.border
                                },
                            ),
                            egui::StrokeKind::Inside,
                        );
                        let label_rect = egui::Rect::from_min_size(
                            rect.min + vec2(6.0, 0.0),
                            vec2(text_width, rect.height()),
                        );
                        let close_rect = egui::Rect::from_min_size(
                            label_rect.right_top(),
                            vec2(Tokens::ICON_BUTTON, rect.height()),
                        );
                        let response = ui.put(
                            label_rect,
                            egui::Button::new(RichText::new(&label).color(if index == active {
                                palette.accent
                            } else {
                                palette.text
                            }))
                            .frame(false)
                            .truncate(),
                        );
                        let close_label = format!("{}: {}", t("Close Tab"), doc.title());
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::Role::Button,
                                ui.is_enabled(),
                                index == active,
                                &label,
                            )
                        });
                        let close = ui
                            .put(
                                close_rect,
                                egui::Button::new(RichText::new(icons::CLOSE).size(16.0))
                                    .frame(false),
                            )
                            .accessible_name(&close_label)
                            .on_hover_text(close_label);
                        (response, close)
                    })
                    .inner;
                let response = response.on_hover_text(
                    doc.path
                        .as_ref()
                        .map_or_else(|| label.clone(), |p| p.display().to_string()),
                );
                if (index == active && active_changed)
                    || response.gained_focus()
                    || close.gained_focus()
                {
                    ui.scroll_to_rect(response.rect.union(close.rect), None);
                }
                if response.clicked() && touch_allowed {
                    selected = Some(index);
                }
                if (response.middle_clicked() || close.clicked() || close.middle_clicked())
                    && touch_allowed
                {
                    close_tab = Some(index);
                }
            }
            new_tab = widgets::icon_button(ui, icons::ADD, t("New Tab"), false).clicked()
                && touch_allowed;
        });
    });
    ui.data_mut(|data| {
        data.insert_temp(
            state_id,
            (
                output.state.clamped_offset().x,
                (output.content_size.x - output.inner_rect.width()).max(0.0),
                active_key,
            ),
        );
    });
    (selected, new_tab, close_tab)
}

pub(crate) fn font_selector(
    ui: &mut Ui,
    fonts: &mut thinoda_ui::fonts::SystemFonts,
    value: &mut Option<String>,
    default_label: &str,
) -> egui::Response {
    font_choice(ui, fonts, value, default_label, true, None)
}

fn font_choice(
    ui: &mut Ui,
    fonts: &mut thinoda_ui::fonts::SystemFonts,
    value: &mut Option<String>,
    default_label: &str,
    generic_monospace: bool,
    mut owned: Option<&mut Vec<egui::Id>>,
) -> egui::Response {
    let response = egui::ComboBox::from_id_salt("font_family")
        .selected_text(match value.as_deref() {
            None => default_label,
            Some("monospace") => t("Monospace"),
            Some(name) => name,
        })
        .width(ui.available_width())
        .truncate()
        .show_ui(ui, |ui| {
            let response = ui.selectable_value(value, None, default_label);
            if let Some(owned) = owned.as_deref_mut() {
                owned.push(response.id);
            }
            if generic_monospace {
                let response = ui.selectable_value(value, Some("monospace".into()), t("Monospace"));
                if let Some(owned) = owned.as_deref_mut() {
                    owned.push(response.id);
                }
            }
            for name in fonts.names() {
                let response = ui.selectable_value(value, Some(name.clone()), name);
                if let Some(owned) = owned.as_deref_mut() {
                    owned.push(response.id);
                }
            }
        })
        .response;
    if let Some(owned) = owned {
        owned.push(response.id);
    }
    response
}

/// Swatch colors offered in the format panel (XMind-like palette).
pub const SWATCHES: [Color32; 12] = [
    Color32::from_rgb(0x2E, 0x7C, 0xF6),
    Color32::from_rgb(0x00, 0xA8, 0x9D),
    Color32::from_rgb(0x2B, 0xB6, 0x73),
    Color32::from_rgb(0xF5, 0xA6, 0x23),
    Color32::from_rgb(0xF2, 0x6B, 0x3A),
    Color32::from_rgb(0xE8, 0x45, 0x5C),
    Color32::from_rgb(0xA3, 0x5B, 0xE0),
    Color32::from_rgb(0x5B, 0x6B, 0xE0),
    Color32::from_rgb(0x33, 0x33, 0x33),
    Color32::from_rgb(0x88, 0x88, 0x88),
    Color32::from_rgb(0xEE, 0xEE, 0xEE),
    Color32::WHITE,
];

pub fn to_color(c: Color32) -> Color {
    Color {
        r: c.r(),
        g: c.g(),
        b: c.b(),
        a: c.a(),
    }
}

pub fn from_color(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn style_color_row(
    ui: &mut Ui,
    label: &'static str,
    selected: Option<Color>,
    show_none: bool,
) -> Option<Color> {
    widgets::property_row(ui, t(label), |ui| {
        if show_none {
            ui.label(t("None"));
        }
        widgets::color_swatches(ui, &SWATCHES, selected.map(from_color)).map(to_color)
    })
}

pub(crate) fn structure_label(s: Structure) -> &'static str {
    match s {
        Structure::MapClockwise => "Mind Map",
        Structure::MapAnticlockwise => "Mind Map (anticlockwise)",
        Structure::MapUnbalanced => "Mind Map (one side)",
        Structure::LogicRight => "Logic Chart (right)",
        Structure::LogicLeft => "Logic Chart (left)",
        Structure::BraceRight => "Brace Map (right)",
        Structure::BraceLeft => "Brace Map (left)",
        Structure::OrgDown => "Org Chart (down)",
        Structure::OrgUp => "Org Chart (up)",
        Structure::TreeRight => "Tree Chart (right)",
        Structure::TreeLeft => "Tree Chart (left)",
        Structure::TimelineHorizontal => "Timeline (horizontal)",
        Structure::TimelineVertical => "Timeline (vertical)",
        Structure::FishboneRight => "Fishbone (right)",
        Structure::FishboneLeft => "Fishbone (left)",
        Structure::Matrix => "Matrix",
        Structure::TreeTable => "Tree Table",
    }
}

impl ThinodaApp {
    fn capture_card_text_command(&self, command: &Cmd) -> CapturedCardTextCommand {
        match command {
            Cmd::CloseTab(index) => {
                CapturedCardTextCommand::CloseTab(self.docs.get(*index).map(|doc| {
                    let owner = self
                        .document_windows
                        .get(&doc.key)
                        .copied()
                        .unwrap_or(egui::ViewportId::ROOT);
                    (doc.doc.instance_id(), owner)
                }))
            }
            command => CapturedCardTextCommand::Command(command.clone()),
        }
    }

    fn resolve_card_text_command(&self, command: CapturedCardTextCommand) -> Option<Cmd> {
        match command {
            CapturedCardTextCommand::Command(command) => Some(command),
            CapturedCardTextCommand::CloseTab(Some((instance, owner))) => self
                .docs
                .iter()
                .position(|doc| {
                    doc.doc.instance_id() == instance
                        && self
                            .document_windows
                            .get(&doc.key)
                            .copied()
                            .unwrap_or(egui::ViewportId::ROOT)
                            == owner
                })
                .map(Cmd::CloseTab),
            CapturedCardTextCommand::CloseTab(None) => None,
        }
    }

    pub(crate) fn request_open_from_ui(
        &mut self,
        ctx: &Context,
        path: &std::path::Path,
        order: CardOpenOrder,
    ) {
        let target = self
            .docs
            .iter()
            .find(|doc| doc.path.as_deref() == Some(path));
        let target_viewport = target
            .and_then(|doc| self.document_windows.get(&doc.key).copied())
            .unwrap_or(egui::ViewportId::ROOT);
        let force = target.is_some()
            && target_viewport != self.current_viewport
            && card_text_input_in_scope(ctx, target_viewport);
        let operation = DeferredCardTextOperation {
            origin: self.current_viewport,
            not_before_root_frame: ctx
                .cumulative_frame_nr_for(egui::ViewportId::ROOT)
                .saturating_add(1),
            action: DeferredCardTextAction::Open {
                path: path.to_owned(),
                target_instance: target.map(|doc| doc.doc.instance_id()),
                order,
            },
        };
        if !self.queue_card_text_operation(ctx, operation, force) {
            self.perform_card_text_open(ctx, path);
        }
    }

    fn queue_card_text_operation(
        &self,
        ctx: &Context,
        operation: DeferredCardTextOperation,
        force: bool,
    ) -> bool {
        let slot = card_text_slot(egui::ViewportId::ROOT).with("deferred_operations");
        let already_waiting = ctx.data(|data| {
            data.get_temp::<Vec<DeferredCardTextOperation>>(slot)
                .is_some_and(|operations| {
                    operations
                        .iter()
                        .any(|item| item.origin == operation.origin)
                })
        });
        if force || already_waiting || self.card_text_operation_blocked(ctx, &operation) {
            ctx.data_mut(|data| {
                let mut operations = data
                    .get_temp::<Vec<DeferredCardTextOperation>>(slot)
                    .unwrap_or_default();
                operations.push(operation);
                data.insert_temp(slot, operations);
            });
            request_card_text_input_repaint(ctx, self.current_viewport);
            true
        } else {
            false
        }
    }

    fn card_text_operation_blocked(
        &self,
        ctx: &Context,
        operation: &DeferredCardTextOperation,
    ) -> bool {
        match &operation.action {
            DeferredCardTextAction::Open { path, order, .. } => {
                if *order == CardOpenOrder::AfterQueuedInput
                    && card_text_input_has_pending_events(ctx, operation.origin)
                {
                    return true;
                }
                self.docs
                    .iter()
                    .find(|doc| doc.path.as_deref() == Some(path.as_path()))
                    .is_some_and(|doc| {
                        let owner = self
                            .document_windows
                            .get(&doc.key)
                            .copied()
                            .unwrap_or(egui::ViewportId::ROOT);
                        owner != operation.origin && card_text_input_has_pending_events(ctx, owner)
                    })
            }
            DeferredCardTextAction::SaveAll => std::iter::once(egui::ViewportId::ROOT)
                .chain(self.document_windows.values().copied())
                .any(|viewport| {
                    viewport != operation.origin
                        && card_text_input_has_pending_events(ctx, viewport)
                }),
            DeferredCardTextAction::Command { command, .. } => {
                card_text_input_has_pending_events(ctx, operation.origin)
                    || (matches!(command, CapturedCardTextCommand::Command(Cmd::SaveAll))
                        && std::iter::once(egui::ViewportId::ROOT)
                            .chain(self.document_windows.values().copied())
                            .any(|viewport| {
                                viewport != operation.origin
                                    && card_text_input_has_pending_events(ctx, viewport)
                            }))
            }
        }
    }

    fn perform_card_text_open(&mut self, ctx: &Context, path: &std::path::Path) {
        let previous = self
            .docs
            .iter()
            .find(|doc| doc.path.as_deref() == Some(path))
            .and_then(|doc| self.document_windows.get(&doc.key).copied());
        self.request_open(path);
        if let Some(viewport) = previous
            && !self
                .document_windows
                .values()
                .any(|owner| *owner == viewport)
        {
            let slot = card_text_slot(viewport).with("deferred_input");
            ctx.data_mut(|data| {
                if data
                    .get_temp::<DeferredCardTextInput>(slot)
                    .is_some_and(|pending| pending.events.is_empty())
                {
                    data.remove::<DeferredCardTextInput>(slot);
                }
            });
        }
    }

    fn finish_deferred_card_text_operations(&mut self, ctx: &Context) {
        let slot = card_text_slot(egui::ViewportId::ROOT).with("deferred_operations");
        let operations = ctx.data_mut(|data| {
            let operations = data
                .get_temp::<Vec<DeferredCardTextOperation>>(slot)
                .unwrap_or_default();
            data.remove::<Vec<DeferredCardTextOperation>>(slot);
            operations
        });
        let mut remaining = Vec::new();
        let mut origin_blocked = false;
        for operation in operations {
            if self.current_viewport == egui::ViewportId::ROOT
                && operation.origin != egui::ViewportId::ROOT
                && !self
                    .document_windows
                    .values()
                    .any(|owner| *owner == operation.origin)
            {
                match &operation.action {
                    DeferredCardTextAction::Open { path, .. } => {
                        self.toasts
                            .warning(format!("{}: {}", t("Could not open"), path.display()))
                    }
                    DeferredCardTextAction::SaveAll | DeferredCardTextAction::Command { .. } => {
                        self.toasts.warning(t("Not possible here"))
                    }
                };
                continue;
            }
            if operation.origin != self.current_viewport {
                remaining.push(operation);
                continue;
            }
            if origin_blocked
                || ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT)
                    < operation.not_before_root_frame
                || self.card_text_operation_blocked(ctx, &operation)
            {
                origin_blocked = true;
                remaining.push(operation);
            } else {
                match operation.action {
                    DeferredCardTextAction::Open {
                        path,
                        target_instance,
                        ..
                    } => {
                        let target_changed = target_instance.is_some_and(|instance| {
                            !self.docs.iter().any(|doc| {
                                doc.path.as_deref() == Some(path.as_path())
                                    && doc.doc.instance_id() == instance
                            })
                        });
                        if target_changed {
                            self.toasts.warning(format!(
                                "{}: {}",
                                t("Could not open"),
                                path.display()
                            ));
                        } else {
                            self.perform_card_text_open(ctx, &path);
                        }
                    }
                    DeferredCardTextAction::SaveAll => {
                        let executing = slot.with("executing");
                        ctx.data_mut(|data| data.insert_temp(executing, true));
                        self.exec(ctx, Cmd::SaveAll);
                        ctx.data_mut(|data| data.remove::<bool>(executing));
                    }
                    DeferredCardTextAction::Command {
                        instance,
                        sheet,
                        root,
                        command,
                    } => {
                        if self.doc().is_some_and(|doc| {
                            doc.doc.instance_id() == instance
                                && doc.doc.active_index() == sheet
                                && doc.doc.sheet().root == root
                        }) && let Some(command) = self.resolve_card_text_command(command)
                        {
                            let executing = slot.with("executing");
                            ctx.data_mut(|data| data.insert_temp(executing, true));
                            self.exec(ctx, command);
                            ctx.data_mut(|data| data.remove::<bool>(executing));
                        } else {
                            self.toasts.warning(t("Not possible here"));
                        }
                    }
                }
                let added = ctx.data_mut(|data| {
                    let added = data
                        .get_temp::<Vec<DeferredCardTextOperation>>(slot)
                        .unwrap_or_default();
                    data.remove::<Vec<DeferredCardTextOperation>>(slot);
                    added
                });
                origin_blocked |= added
                    .iter()
                    .any(|item| item.origin == self.current_viewport);
                remaining.extend(added);
                ctx.request_repaint();
            }
        }
        if !remaining.is_empty() {
            ctx.data_mut(|data| data.insert_temp(slot, remaining));
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    pub(crate) fn prepare_card_text_input(&self, ctx: &Context, input: &mut egui::RawInput) {
        let viewport = input.viewport_id;
        self.mark_card_text_input_scope(ctx, viewport);
        if viewport != egui::ViewportId::ROOT {
            return;
        }
        let slot = card_text_slot(viewport);
        let mut children: Vec<_> = self.document_windows.values().copied().collect();
        children.sort_unstable();
        children.dedup();
        for &child in &children {
            self.mark_card_text_input_scope(ctx, child);
        }
        ctx.data_mut(|data| data.insert_temp(slot.with("document_viewports"), children.clone()));
        if !input.viewport().close_requested() {
            return;
        }
        let authorized = ctx.data_mut(|data| {
            let authorized = data
                .get_temp::<bool>(slot.with("authorized_close"))
                .unwrap_or(false);
            data.remove::<bool>(slot.with("authorized_close"));
            authorized
        });
        if authorized {
            return;
        }
        let scoped_or_pending = std::iter::once(viewport).chain(children).any(|id| {
            let slot = card_text_slot(id);
            ctx.data(|data| {
                data.get_temp::<bool>(slot.with("input_scope"))
                    .unwrap_or(false)
                    || data
                        .get_temp::<DeferredCardTextInput>(slot.with("deferred_input"))
                        .is_some()
            })
        });
        if scoped_or_pending {
            ctx.input_for(viewport, |_| ());
            let frame = ctx.cumulative_frame_nr_for(viewport);
            ctx.data_mut(|data| {
                let mut deferred = data
                    .get_temp::<DeferredCardTextInput>(slot.with("deferred_input"))
                    .unwrap_or_default();
                deferred.close_after_frame.get_or_insert(frame + 1);
                data.insert_temp(slot.with("deferred_input"), deferred);
            });
            if let Some(info) = input.viewports.get_mut(&viewport) {
                info.events
                    .retain(|event| *event != egui::ViewportEvent::Close);
            }
            request_card_text_input_repaint(ctx, viewport);
        }
    }

    fn mark_card_text_input_scope(&self, ctx: &Context, viewport: egui::ViewportId) {
        let visible = self.visible_document_indices(viewport);
        let active = self
            .window_active_documents
            .get(&viewport)
            .and_then(|key| self.docs.iter().position(|doc| &doc.key == key))
            .filter(|index| visible.contains(index))
            .or_else(|| visible.contains(&self.active).then_some(self.active))
            .or_else(|| visible.first().copied());
        let eligible = active
            .and_then(|index| self.docs.get(index))
            .is_some_and(|doc| {
                let topic = doc.view.editing_topic().or_else(|| {
                    (doc.view.selection.len() == 1
                        && doc.view.board_selection.is_empty()
                        && doc.view.selected_object.is_none()
                        && doc.view.relationship_from.is_none())
                    .then(|| doc.view.primary())
                    .flatten()
                });
                topic.is_some_and(|id| {
                    doc.doc
                        .sheet()
                        .topic(id)
                        .and_then(|topic| topic.card)
                        .is_some_and(|card| matches!(card.kind, CardKind::Sticky | CardKind::Text))
                })
            });
        ctx.data_mut(|data| {
            data.insert_temp(card_text_slot(viewport).with("input_scope"), eligible)
        });
    }

    pub(crate) fn ribbon_active(&self) -> bool {
        #[cfg(target_os = "windows")]
        return self.desktop_ribbon;
        #[cfg(target_os = "macos")]
        return self.native_menu_active();
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        false
    }

    /// Draws one frame of the whole application.
    pub fn draw(&mut self, ui: &mut Ui) {
        thinoda_ui::fonts::ensure_icon_font(ui.ctx());
        widgets::touch_activation_allowed(ui.ctx());
        if !self.launch_frame(ui) {
            return;
        }
        self.poll_clipboard(ui.ctx());
        let ctx = ui.ctx().clone();
        let viewport = if self.drawing_document_window {
            self.current_viewport
        } else {
            ctx.viewport_id()
        };
        let previous_active = self.active;
        let previous_viewport = self.current_viewport;
        self.current_viewport = viewport;
        self.activate_document_window(viewport);
        self.mark_card_text_input_scope(&ctx, viewport);
        let frame = ctx.cumulative_frame_nr();
        ctx.data_mut(|data| {
            data.insert_temp(card_text_slot(viewport).with("before_editor"), frame);
        });
        let visible_doc_before_chrome = self.doc().map(|doc| doc.key.clone());
        self.guard_table_draft(&ctx);
        self.prepare_keymap(&ctx);
        // Menus can dismiss an open popup before its own input field is drawn.
        self.protect_labels_ime(&ctx);
        self.protect_ai_prompt_input(&ctx);
        self.protect_drawio_link_ime(&ctx);
        if self.palette.open {
            let id = egui::Id::unique("thinoda_command_palette_input");
            if self.dialogs.palette_composing || ctx.memory(|memory| memory.has_focus(id)) {
                thinoda_canvas::protect_ime_candidate_keys(
                    &ctx,
                    id,
                    &mut self.dialogs.palette_composing,
                );
            }
        } else {
            self.dialogs.palette_composing = false;
        }
        if let Some(draft) = &mut self.dialogs.rename_sheet {
            thinoda_canvas::protect_ime_candidate_keys(
                &ctx,
                egui::Id::unique("thinoda_rename_input"),
                &mut draft.ime_composing,
            );
            if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                self.dialogs.rename_sheet = None;
            }
        }
        let font_source = self.doc().map(|doc| (doc.key.clone(), doc.doc.state_id()));
        let preferred_fallback = (!crate::files::is_web())
            .then_some(self.settings.default_cjk_font.as_deref())
            .flatten();
        if font_source != self.font_source || self.fonts.preferred_fallback() != preferred_fallback
        {
            let families: std::collections::BTreeSet<_> =
                self.doc()
                    .into_iter()
                    .flat_map(|doc| doc.doc.workbook().sheets.iter())
                    .flat_map(|sheet| {
                        sheet.theme.fonts().chain(
                            sheet
                                .topics
                                .values()
                                .filter_map(|topic| topic.style.font_family.as_ref()),
                        )
                    })
                    .chain(
                        self.settings
                            .quick_styles
                            .iter()
                            .filter_map(|style| style.style.font_family.as_ref()),
                    )
                    .chain(
                        self.settings.custom_themes.iter().flat_map(|custom| {
                            custom.font_family.iter().chain(custom.roles.fonts())
                        }),
                    )
                    .cloned()
                    .collect();
            for name in self.fonts.ensure(&ctx, families, preferred_fallback) {
                log::warn!("Font unavailable: {name}");
                self.toasts.warning(format!(
                    "{}: {name}. {}",
                    t("Font unavailable"),
                    t("Using the default font; the requested name is preserved.")
                ));
            }
            self.font_source = font_source;
        }
        self.apply_appearance(&ctx);
        if viewport == egui::ViewportId::ROOT {
            self.updates.show(ui);
        }
        self.handle_close_request(&ctx);
        self.handle_sheet_paste(&ctx);
        self.poll_map_shot(&ctx);
        let popup_was_open = egui::Popup::is_any_open(&ctx);
        if !self.palette.open && !self.any_modal_open() && !popup_was_open {
            self.global_shortcuts(&ctx);
            self.insert_shortcuts(&ctx);
        }

        let visible_documents = self.visible_document_indices(viewport);
        let show_doc = visible_documents.contains(&self.active);
        if !self.zen {
            egui::Panel::top("thinoda_topbar")
                .exact_size(if self.ribbon_active() {
                    56.0
                } else {
                    Tokens::TOOLBAR_HEIGHT
                })
                .frame(
                    egui::Frame::new()
                        .fill(Palette::of(&ctx).surface)
                        .inner_margin(Margin::symmetric(8, 6)),
                )
                .show(ui, |ui| {
                    if self.palette.open
                        || self.fim_hierarchy_review_open()
                        || self.planning.editing()
                        || self.zones.editing()
                        || self.pitch.blocks_map()
                        || self.map_shot.active()
                        || self.printing.active()
                        || self.dialogs.legal.open
                        || self.conversion_modal_open()
                        || self.dialogs.symbol_export.is_some()
                    {
                        ui.disable();
                    }
                    self.top_bar(ui);
                });
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            if self.ribbon_active() {
                self.show_ribbon(ui);
            }
            if show_doc {
                egui::Panel::bottom("thinoda_statusbar")
                    .exact_size(30.0)
                    .frame(
                        egui::Frame::new()
                            .fill(Palette::of(&ctx).surface)
                            .inner_margin(Margin::symmetric(8, 3)),
                    )
                    .show(ui, |ui| {
                        if self.palette.open
                            || self.fim_hierarchy_review_open()
                            || self.map_shot.active()
                            || self.printing.active()
                            || self.dialogs.legal.open
                            || self.conversion_modal_open()
                            || self.dialogs.symbol_export.is_some()
                        {
                            ui.disable();
                        }
                        self.bottom_bar(ui);
                    });
                if self.ai.chat.open {
                    egui::Panel::right("thinoda_ai_chat")
                        .default_size(320.0)
                        .size_range(240.0..=480.0)
                        .frame(
                            egui::Frame::new()
                                .fill(Palette::of(&ctx).surface)
                                .inner_margin(Margin::same(10)),
                        )
                        .show(ui, |ui| {
                            if self.any_modal_open()
                                || self.palette.open
                                || self.map_shot.active()
                                || self.printing.active()
                                || self.dialogs.legal.open
                                || self.conversion_modal_open()
                            {
                                ui.disable();
                            }
                            self.ai_chat_panel(ui);
                        });
                } else if self.settings.show_format_panel {
                    egui::Panel::right("thinoda_format")
                        .default_size(280.0)
                        .size_range(240.0..=420.0)
                        .frame(
                            egui::Frame::new()
                                .fill(Palette::of(&ctx).surface)
                                .inner_margin(Margin::same(10)),
                        )
                        .show(ui, |ui| {
                            if self.palette.open
                                || self.fim_hierarchy_review_open()
                                || self.map_shot.active()
                                || self.printing.active()
                                || self.dialogs.legal.open
                                || self.conversion_modal_open()
                                || self.dialogs.symbol_export.is_some()
                            {
                                ui.disable();
                            }
                            self.sidebar(ui);
                        });
                }
            }
        }

        if popup_was_open && !egui::Popup::is_any_open(&ctx) {
            ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
        }
        if self.search.open
            && !self.palette.open
            && !self.any_modal_open()
            && !self.dialogs.settings
            && !popup_was_open
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.search.open = false;
            self.apply_search_filter();
        }
        if visible_doc_before_chrome.as_ref() != self.doc().map(|doc| &doc.key)
            && let Some(previous) = visible_doc_before_chrome
            && let Some(doc) = self.docs.iter_mut().find(|doc| doc.key == previous)
        {
            doc.suspend_ink(&ctx);
        }
        if show_doc
            && (self.mode != CenterMode::Map || self.task_table_visible())
            && let Some(doc) = self.doc_mut()
        {
            doc.suspend_ink(&ctx);
        }
        for &index in &visible_documents {
            if index != self.active
                && let Some(doc) = self.docs.get_mut(index)
            {
                doc.suspend_ink(&ctx);
            }
        }

        let card_text_open = if show_doc
            && self.mode == CenterMode::Map
            && !self.task_table_visible()
            && !self.search.open
            && !self.palette.open
            && !self.any_modal_open()
            && !self.dialogs.settings
            && !self.dialogs.about
        {
            self.card_text_toolbar(ui)
        } else {
            ctx.data_mut(|data| {
                data.remove::<CardTextTarget>(card_text_slot(self.current_viewport));
            });
            false
        };

        let center = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if self.palette.open
                    || self.fim_hierarchy_review_open()
                    || self.planning.editing()
                    || self.zones.editing()
                    || self.pitch.blocks_map()
                    || self.label_editor.is_some()
                    || self.printing.active()
                    || self.quick_style_dialog.is_some()
                    || self.theme_dialog.is_some()
                    || self.ai.prompt_editor.is_some()
                    || self.ai.prompt.is_some()
                    || self.dialogs.guide.open
                    || self.dialogs.shortcuts
                    || self.dialogs.legal.open
                    || self.conversion_modal_open()
                    || self.dialogs.symbol_export.is_some()
                {
                    ui.disable();
                }
                if !show_doc {
                    self.home(ui);
                } else if self.mode == CenterMode::Outliner {
                    self.outliner(ui);
                } else if self.task_table_visible() {
                    self.tree_table_ui(ui);
                } else {
                    if self
                        .doc()
                        .is_some_and(|doc| doc.doc.sheet().structure == Structure::TreeTable)
                        && ui.button(t("Tree Table")).clicked()
                        && widgets::touch_activation_allowed(&ctx)
                    {
                        self.tree_table.map_preview = false;
                    }
                    self.map(ui);
                }
            });
        self.finish_card_text_focus_handoff(&ctx);
        self.finish_deferred_card_text_commands(&ctx);

        let (board_toolbar, editing_toolbar) = if show_doc
            && self.mode == CenterMode::Map
            && !self.task_table_visible()
            && !self.zen
            && !self.any_modal_open()
            && !self.dialogs.settings
        {
            let editing_toolbar = if card_text_open {
                None
            } else {
                self.editing_toolbar(&ctx, center.response.rect)
            };
            let board_toolbar = self.board_toolbar(&ctx, center.response.rect, editing_toolbar);
            self.armed_shape_hint(&ctx, center.response.rect);
            (board_toolbar, editing_toolbar)
        } else {
            Self::close_connector_popups(&ctx, self.current_viewport);
            (None, None)
        };

        if show_doc
            && self.mode == CenterMode::Map
            && !self.task_table_visible()
            && !self.search.open
            && !self.palette.open
            && !self.any_modal_open()
            && !self.dialogs.settings
        {
            self.topic_toolbar(&ctx, board_toolbar, editing_toolbar);
        }

        if show_doc && self.search.open && !self.map_shot.active() && !self.printing.active() {
            self.search_bar(&ctx);
        }
        if show_doc && !self.any_modal_open() && !self.dialogs.settings {
            self.ink_recovery_ui(&ctx);
        }
        self.update_search();

        // Command palette.
        let mut cmds = self.command_catalog(&ctx);
        self.add_quick_style_commands(&mut cmds);
        self.add_custom_theme_commands(&mut cmds);
        let list: Vec<thinoda_ui::Command> = cmds
            .iter()
            .filter(|(cmd, _)| self.command_available(cmd))
            .map(|(_, c)| c.clone())
            .collect();
        if let Some(id) = self.palette.show(&ctx, &list, t("Type a command…"))
            && let Some((cmd, _)) = cmds.iter().find(|(_, c)| c.id == id)
        {
            self.exec(&ctx, cmd.clone());
        }

        self.labels_popup(&ctx);
        self.quick_style_dialog(&ctx);
        self.theme_editor_dialog(&ctx);
        self.guide_dialog(&ctx);
        self.shortcuts_dialog(&ctx);
        let board_modal_was_open = self.dialogs.board_export.is_some();
        if !self.map_shot.active() {
            self.dialogs_ui(&ctx);
            self.ai_prompt_dialog(&ctx);
            self.ai_busy_overlay(&ctx);
            self.file_busy_overlay(&ctx);
            self.ai_review_window(&ctx);
            self.saved_prompt_dialog(&ctx);
        }
        self.map_shot_dialog(&ctx);
        if !board_modal_was_open && self.dialogs.board_export.is_none() {
            self.print_dialog(&ctx);
        }
        self.planning_dialog(&ctx);
        self.zones_window(&ctx);
        self.zone_dialog(&ctx);
        if !board_modal_was_open && self.dialogs.board_export.is_none() {
            self.presentation_dialog(&ctx);
        }
        self.legal_dialog(&ctx);
        if !self.map_shot.requested && !self.map_shot.capturing {
            let modal_open = self.any_modal_open();
            if self.dialogs.settings
                && !modal_open
                && let Some(strip) = self.toasts.bottom_strip(&ctx)
            {
                self.toasts.show_in_bottom_strip(&ctx, strip);
            } else {
                self.toasts.show_with_modal(&ctx, modal_open);
            }
        }
        if let Some(d) = self.doc() {
            self.capture_map_shot(&ctx, d.view.canvas_rect());
        }
        self.housekeeping(&ctx);
        if viewport == egui::ViewportId::ROOT && !self.drawing_document_window {
            self.draw_document_windows(&ctx);
            self.gantt_window(&ctx);
            #[cfg(not(target_arch = "wasm32"))]
            self.pitch_window(&ctx);
            #[cfg(target_arch = "wasm32")]
            if !board_modal_was_open && self.dialogs.board_export.is_none() {
                self.pitch_window(&ctx);
            }
        } else {
            let returned_home = viewport != egui::ViewportId::ROOT
                && self.active == self.docs.len()
                && !self
                    .document_windows
                    .values()
                    .any(|owner| *owner == viewport);
            if self.active < self.docs.len()
                && self
                    .visible_document_indices(viewport)
                    .contains(&self.active)
                && let Some(doc) = self.docs.get(self.active)
            {
                self.window_active_documents
                    .insert(viewport, doc.key.clone());
            }
            if !returned_home {
                self.active = previous_active;
            }
        }
        self.finish_deferred_card_text_operations(&ctx);
        self.mark_card_text_input_scope(&ctx, viewport);
        if card_text_input_needs_repaint(&ctx, viewport) {
            request_card_text_input_repaint(&ctx, viewport);
        }
        self.current_viewport = previous_viewport;
    }

    fn draw_document_windows(&mut self, ctx: &egui::Context) {
        let mut seen = std::collections::HashSet::new();
        let windows: Vec<_> = self
            .document_windows
            .iter()
            .filter_map(|(key, viewport)| {
                if !seen.insert(*viewport) {
                    return None;
                }
                let active_key = self
                    .window_active_documents
                    .get(viewport)
                    .filter(|active| self.document_windows.get(*active) == Some(viewport))
                    .cloned()
                    .unwrap_or_else(|| key.clone());
                let title = self
                    .docs
                    .iter()
                    .find(|doc| doc.key == active_key)
                    .map(|doc| {
                        doc.path
                            .as_ref()
                            .and_then(|path| path.file_name())
                            .map_or_else(|| doc.title(), |name| name.to_string_lossy().into_owned())
                    })
                    .unwrap_or_else(|| t("Untitled").to_owned());
                Some((*viewport, active_key, title))
            })
            .collect();

        for (viewport, active_key, title) in windows {
            let builder = egui::ViewportBuilder::default()
                .with_title(format!("{title} — Thinoda"))
                .with_inner_size([1280.0, 800.0])
                .with_min_inner_size([720.0, 480.0]);
            let previous_viewport = self.current_viewport;
            let previous_active = self.active;
            self.current_viewport = viewport;
            self.drawing_document_window = true;
            self.window_active_documents
                .insert(viewport, active_key.clone());
            self.mark_card_text_input_scope(ctx, viewport);
            ctx.show_viewport_immediate(viewport, builder, |ui, _class| {
                self.activate_document_window(viewport);
                #[cfg(target_os = "macos")]
                let native_menu_focused = ui
                    .ctx()
                    .input(|input| input.viewport().focused.unwrap_or(false));
                #[cfg(target_os = "macos")]
                if native_menu_focused {
                    self.poll_native_menus(ui.ctx());
                }
                self.draw(ui);
                #[cfg(target_os = "macos")]
                if native_menu_focused {
                    self.sync_native_menus(ui.ctx());
                }
            });
            self.drawing_document_window = false;
            self.current_viewport = previous_viewport;
            let returned_home = !self
                .document_windows
                .values()
                .any(|owner| *owner == viewport)
                && self.active == self.docs.len();
            if !returned_home {
                self.active = previous_active;
            }
        }
    }

    fn conversion_modal_open(&self) -> bool {
        self.dialogs.mindmanager_export
            || self.dialogs.note_export.is_some()
            || self.dialogs.powerpoint_export
            || self.dialogs.mindmanager_save.is_some()
            || self.dialogs.board_export.is_some()
            || self.dialogs.drawio_link.is_some()
            || !self.dialogs.imports.is_empty()
    }

    pub(crate) fn any_modal_open(&self) -> bool {
        if self.dialogs.masked_copy
            || self.cloud_program.modal_open()
            || self.cloud_changes.modal_open()
            || self.fim_node_modal_open()
            || self.fim_hierarchy_review_open()
            || self.fim.consent.is_some()
            || self.planning.editing()
            || self.zones.editing()
            || self.pitch.blocks_map()
            || self.printing.active()
            || self.map_shot.active()
            || self.label_editor.is_some()
            || self.quick_style_dialog.is_some()
            || self.theme_dialog.is_some()
            || self.dialogs.guide.open
            || self.dialogs.shortcuts
            || self.dialogs.legal.open
            || self.dialogs.symbol_export.is_some()
            || self.dialogs.note_export.is_some()
            || self.dialogs.board_export.is_some()
            || self.dialogs.drawio_link.is_some()
        {
            return true;
        }
        let d = &self.dialogs;
        d.close.is_some()
            || d.mindmanager_save.is_some()
            || d.mindmanager_export
            || !d.imports.is_empty()
            || d.conflict.is_some()
            || d.rename_sheet.is_some()
            || !d.recovery.is_empty()
            || self.ai.prompt.is_some()
            || self.ai.prompt_editor.is_some()
    }

    // ----- top bar ----------------------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut cmd: Option<Cmd> = None;
        let touch_allowed = widgets::touch_activation_allowed(&ctx);
        let visible_documents = self.visible_document_indices(self.current_viewport);
        let inline_menu_bar = !cfg!(target_arch = "wasm32")
            && ctx.os() == egui::os::OperatingSystem::Nix
            && ctx.content_rect().width() >= 1400.0 * self.settings.ui_scale.clamp(0.5, 3.0);
        ui.horizontal_centered(|ui| {
            if inline_menu_bar {
                self.main_menu_bar(ui, &mut cmd);
            } else if !cfg!(target_os = "windows") || !self.ribbon_active() {
                guarded_menu_button(
                    ui,
                    RichText::new(icons::MENU).size(icons::SIZE_TOOLBAR),
                    |ui| self.main_menu(ui, &mut cmd),
                )
                .response
                .accessible_name(t("Main Menu"))
                .on_hover_text(t("Main Menu"));
            }
            let header_width = ctx.content_rect().width() * ctx.zoom_factor();
            let compact_header = header_width < 640.0;
            let very_compact_header = header_width < 480.0;
            let (can_undo, can_redo) = self.doc().map_or((false, false), |d| {
                let draft_changed = d.has_inspector_changes();
                (
                    d.doc.can_undo() || draft_changed,
                    d.doc.can_redo() && !draft_changed,
                )
            });
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            if self.ribbon_active() {
                self.quick_access_toolbar(ui);
            }
            if !self.ribbon_active() && !very_compact_header {
                if ui
                    .add_enabled_ui(can_undo, |ui| {
                        widgets::icon_button(ui, icons::UNDO, t("Undo"), false)
                    })
                    .inner
                    .clicked()
                {
                    cmd = Some(Cmd::Canvas(Action::Undo));
                }
                if ui
                    .add_enabled_ui(can_redo, |ui| {
                        widgets::icon_button(ui, icons::REDO, t("Redo"), false)
                    })
                    .inner
                    .clicked()
                {
                    cmd = Some(Cmd::Canvas(Action::Redo));
                }
            }
            widgets::vdivider(ui);
            let tabs_width = if header_width < 600.0 {
                ui.available_width().max(0.0)
            } else {
                let arrow_slot = ui.spacing().interact_size.y + ui.spacing().item_spacing.x;
                let action_slot = Tokens::ICON_BUTTON + ui.spacing().item_spacing.x;
                let min_tabs_width = 2.0 * arrow_slot + 2.0 * action_slot;
                (ui.available_width() * 0.4).max(min_tabs_width).min(560.0)
            };
            let (selected, new_tab, close_tab) = document_tabs(
                ui,
                tabs_width,
                &self.docs,
                &visible_documents,
                self.current_viewport,
                self.active,
            );
            if touch_allowed {
                if let Some(index) = selected {
                    self.active = index;
                }
                if let Some(index) = close_tab {
                    cmd = Some(Cmd::CloseTab(index));
                }
                if new_tab {
                    cmd = Some(Cmd::Home);
                }
            }

            if header_width >= 600.0 {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let format = widgets::icon_button(
                        ui,
                        icons::FORMAT,
                        t("Format Panel"),
                        self.doc().is_some()
                            && self.settings.show_format_panel
                            && self.sidebar_family == SidebarFamily::Format,
                    );
                    if format.clicked() {
                        cmd = Some(Cmd::ToggleFormatPanel);
                    }
                    let symbols = widgets::icon_button(
                        ui,
                        icons::MARKER,
                        t("Symbols"),
                        self.doc().is_some()
                            && self.settings.show_format_panel
                            && self.sidebar_family == SidebarFamily::Symbols,
                    );
                    if symbols.clicked() {
                        cmd = Some(Cmd::ToggleSymbolsPanel);
                    }
                    if !compact_header {
                        ui.add_enabled_ui(
                            EXPORT_MENU
                                .iter()
                                .any(|f| self.command_available(&Cmd::Export(*f))),
                            |ui| {
                                guarded_menu_button(ui, RichText::new(icons::EXPORT), |ui| {
                                    for f in EXPORT_MENU {
                                        if ui
                                            .add_enabled(
                                                self.command_available(&Cmd::Export(*f)),
                                                egui::Button::new(t(f.label())),
                                            )
                                            .clicked()
                                        {
                                            cmd = Some(Cmd::Export(*f));
                                        }
                                    }
                                })
                                .response
                                .accessible_name(t("Export"))
                                .on_hover_text(t("Export"));
                            },
                        );
                        if widgets::icon_button(ui, icons::ZEN, t("Zen Mode"), self.zen).clicked() {
                            cmd = Some(Cmd::ToggleZen);
                        }
                    }
                    let outl = self.mode == CenterMode::Outliner;
                    // A toggle: the same Outliner icon as the command, highlighted while active.
                    let outliner = widgets::icon_button(ui, icons::OUTLINER, t("Outliner"), outl);
                    if outliner.clicked() {
                        cmd = Some(Cmd::ToggleOutliner);
                    }
                    if !compact_header
                        && widgets::icon_button(ui, icons::SEARCH, t("Find…"), self.search.open)
                            .clicked()
                    {
                        cmd = Some(Cmd::Find);
                    }
                });
            }
        });
        if touch_allowed && let Some(c) = cmd {
            self.exec(&ctx, c);
        }
    }

    fn main_menu(&mut self, ui: &mut Ui, cmd: &mut Option<Cmd>) {
        self.render_menu_entries(ui, &self.main_menu_entries(), cmd);
    }

    fn main_menu_bar(&mut self, ui: &mut Ui, cmd: &mut Option<Cmd>) {
        for entry in self.main_menu_entries() {
            if let crate::menus::MenuEntry::Submenu { label, entries } = entry {
                let enabled = self.menu_entries_available(&entries);
                ui.add_enabled_ui(enabled, |ui| {
                    menu_button(ui, label, |ui| self.render_menu_entries(ui, &entries, cmd));
                });
            }
        }
    }

    pub(crate) fn render_menu_entries(
        &mut self,
        ui: &mut Ui,
        entries: &[crate::menus::MenuEntry],
        cmd: &mut Option<Cmd>,
    ) {
        use crate::menus::MenuEntry;
        ui.set_min_width(220.0);
        for entry in entries {
            match entry {
                MenuEntry::Command { label, command } => {
                    let shortcut = Self::shortcut_hint_for_mode(ui.ctx(), command, self.mode);
                    let response = ui
                        .add_enabled_ui(self.command_available(command), |ui| {
                            thinoda_ui::ribbon::menu_item(
                                ui,
                                Some(crate::command_icons::command_glyph(command)),
                                label,
                                shortcut,
                            )
                        })
                        .inner;
                    let response = if let Cmd::OpenPath(path) = command {
                        response.on_hover_text(path.display().to_string())
                    } else {
                        response
                    };
                    if response.clicked() && widgets::touch_activation_allowed(ui.ctx()) {
                        *cmd = Some(command.clone());
                        ui.close();
                    }
                }
                MenuEntry::Submenu { label, entries } => {
                    ui.add_enabled_ui(self.menu_entries_available(entries), |ui| {
                        menu_button(ui, label, |ui| self.render_menu_entries(ui, entries, cmd));
                    });
                }
                MenuEntry::Separator => {
                    ui.separator();
                }
                MenuEntry::Caption(text) => {
                    widgets::caption(ui, text);
                }
                MenuEntry::ClearRecent => {
                    if ui.button(t("Clear Recent")).clicked()
                        && widgets::touch_activation_allowed(ui.ctx())
                    {
                        self.settings.clear_recent();
                        self.save_settings();
                    }
                }
            }
        }
    }

    fn marker_menu(&mut self, ui: &mut Ui, cmd: &mut Option<Cmd>) {
        ui.set_max_width(320.0);
        self.symbol_insertion_color(ui);
        for g in markers::GROUPS {
            ui.label(RichText::new(t(g.name)).small().weak());
            ui.horizontal_wrapped(|ui| {
                for m in g.markers {
                    if self.symbol_button(ui, m, false) {
                        *cmd = Some(Cmd::Canvas(Action::ToggleMarkerColor(
                            m.id.to_string(),
                            self.symbol_color,
                        )));
                    }
                }
            });
        }
        self.placed_symbols(ui, cmd);
    }

    // ----- center -------------------------------------------------------------------------------

    fn editing_toolbar(&mut self, ctx: &Context, canvas: egui::Rect) -> Option<egui::Rect> {
        let d = self.doc()?;
        let has_selection = !d.view.selection.is_empty();
        let toolbar_id = egui::Id::unique("thinoda_editing_toolbar");
        let editing_card = d
            .view
            .editing_topic()
            .and_then(|id| d.doc.sheet().topic(id))
            .and_then(|topic| topic.card)
            .is_some_and(|card| matches!(card.kind, CardKind::Sticky | CardKind::Text));
        if editing_card {
            let editor = ctx.read_response(MapCanvas::title_editor_id())?.rect;
            let size = ctx.memory(|memory| memory.area_rect(toolbar_id))?.size();
            if size.x > canvas.width() || size.y > canvas.height() {
                return None;
            }
            let left =
                (canvas.center().x - size.x * 0.5).clamp(canvas.left(), canvas.right() - size.x);
            let top = (canvas.top() + 12.0).clamp(canvas.top(), canvas.bottom() - size.y);
            let clearance = 2.0 / ctx.pixels_per_point() + 1.0;
            if egui::Rect::from_min_size(egui::pos2(left, top), size)
                .expand(clearance)
                .intersects(editor)
            {
                return None;
            }
        }
        let mut cmd = None;
        let rect = egui::Area::new(toolbar_id)
            .order(egui::Order::Middle)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(canvas.center_top() + vec2(0.0, 12.0))
            .constrain_to(canvas)
            .movable(false)
            .show(ctx, |ui| {
                widgets::card(ui).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(has_selection, |ui| {
                            for (icon, label, action) in [
                                (icons::TOPIC, "Topic", Action::InsertSibling),
                                (icons::SUBTOPIC, "Subtopic", Action::InsertChild),
                                (
                                    icons::RELATIONSHIP,
                                    "Relationship",
                                    Action::StartRelationship,
                                ),
                                (icons::BOUNDARY, "Boundary", Action::AddBoundary),
                                (icons::SUMMARY, "Summary", Action::AddSummary),
                            ] {
                                if widgets::icon_button(ui, icon, t(label), false).clicked() {
                                    cmd = Some(Cmd::Canvas(action));
                                }
                            }
                        });
                        widgets::vdivider(ui);
                        menu_button(ui, icons::MARKER, |ui| self.marker_menu(ui, &mut cmd))
                            .response
                            .accessible_name(t("Marker"))
                            .on_hover_text(t("Marker"));
                        menu_button(
                            ui,
                            format!("{} {}", icons::ADD, icons::CHEVRON_DOWN),
                            |ui| self.more_insert_actions(ui, &mut cmd, has_selection),
                        )
                        .response
                        .accessible_name(t("More insert actions"))
                        .on_hover_text(t("More insert actions"));
                    });
                });
            })
            .response
            .rect;
        if let Some(command) = cmd {
            self.exec(ctx, command);
        }
        Some(rect)
    }

    fn more_insert_actions(&mut self, ui: &mut Ui, cmd: &mut Option<Cmd>, has_selection: bool) {
        ui.set_min_width(190.0);
        for (icon, label, enabled, command) in [
            (
                icons::NOTE,
                "Notes",
                self.command_available(&Cmd::EditField(FocusField::Notes)),
                Cmd::EditField(FocusField::Notes),
            ),
            (
                icons::LINK,
                "Hyperlink",
                self.command_available(&Cmd::EditField(FocusField::Hyperlink)),
                Cmd::EditField(FocusField::Hyperlink),
            ),
            (
                icons::LABEL,
                "Labels",
                self.command_available(&Cmd::Labels),
                Cmd::Labels,
            ),
            (
                icons::IMAGE,
                "Local Image",
                self.command_available(&Cmd::LocalImage),
                Cmd::LocalImage,
            ),
            (
                icons::NEW_FILE,
                "Attachment",
                self.command_available(&Cmd::Attachment),
                Cmd::Attachment,
            ),
            (
                icons::SAVE_AS,
                "Save/Download attachment",
                self.command_available(&Cmd::SaveAttachment),
                Cmd::SaveAttachment,
            ),
            (
                icons::TODO,
                "To-Do",
                has_selection,
                Cmd::Canvas(Action::ToggleTodo),
            ),
            (icons::FORMAT, "Topic style", has_selection, Cmd::TopicStyle),
        ] {
            if ui
                .add_enabled(enabled, egui::Button::new(format!("{icon} {}", t(label))))
                .accessible_name(t(label))
                .clicked()
                && widgets::touch_activation_allowed(ui.ctx())
            {
                *cmd = Some(command);
                ui.close();
            }
        }
        ui.separator();
        if ui.button(t("Find and Replace…")).clicked()
            && widgets::touch_activation_allowed(ui.ctx())
        {
            *cmd = Some(Cmd::FindReplace);
            ui.close();
        }
    }

    fn formatting_insert_actions(&mut self, ui: &mut Ui, cmd: &mut Option<Cmd>) {
        let has_selection = self.doc().is_some_and(|doc| !doc.view.selection.is_empty());
        for (icon, label, action) in [
            (icons::TOPIC, "Topic", Action::InsertSibling),
            (icons::SUBTOPIC, "Subtopic", Action::InsertChild),
            (
                icons::RELATIONSHIP,
                "Relationship",
                Action::StartRelationship,
            ),
            (icons::BOUNDARY, "Boundary", Action::AddBoundary),
            (icons::SUMMARY, "Summary", Action::AddSummary),
        ] {
            if ui
                .add_enabled(
                    has_selection,
                    egui::Button::new(format!("{icon} {}", t(label))),
                )
                .accessible_name(t(label))
                .clicked()
                && widgets::touch_activation_allowed(ui.ctx())
            {
                *cmd = Some(Cmd::Canvas(action));
                ui.close();
            }
        }
        let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
        ui.add_enabled_ui(touch_allowed, |ui| {
            menu_button(ui, format!("{} {}", icons::MARKER, t("Marker")), |ui| {
                self.marker_menu(ui, cmd);
            })
            .response
            .accessible_name(t("Marker"));
        });
        ui.separator();
        self.more_insert_actions(ui, cmd, has_selection);
    }

    fn map(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if self.palette.open || self.any_modal_open() || self.dialogs.settings || self.dialogs.about
        {
            if let Some(d) = self.docs.get_mut(self.active) {
                d.suspend_ink(&ctx);
                MapCanvas::show_snapshot(ui, &d.doc, &mut d.view);
            }
            return;
        }
        let shortcuts = self.active_canvas_bindings(ctx.os());
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        crate::board_ui::configure_board_labels(&ctx, &d.key, &mut d.view);
        d.view.add_control_labels.child = t("Add child topic").into();
        d.view.add_control_labels.sibling = t("Add sibling topic").into();
        d.view.image_resize_label = t("Resize image").into();
        d.view.relationship_handle_labels = [
            "Relationship start",
            "First curve control",
            "Second curve control",
            "Relationship end",
        ]
        .map(|label| t(label).to_string());
        d.view.show_add_controls = self.settings.show_topic_add_buttons;
        d.view.show_alignment_guides = self.settings.show_alignment_guides;
        d.view.host_handles_clipboard = true;
        let (out, preview, clipboard_requests, clipboard_text, deferred_shortcut) = {
            let fim = d.doc.sheet().fim.is_some();
            let canvas_focused = fim && fim_canvas_owns_keyboard(&ctx, &d.view);
            // Board and ink gestures remember the actual sheet allocation.
            // Passing a cloned preview on release would cancel the gesture
            // (or strand its ink samples as stale). Keep those interactions
            // on the live document; source-changing shortcuts are handed
            // back to the FIM-aware command path below.
            let board_context = fim
                && (d.view.board_tool != BoardTool::Select
                    || !d.view.board_selection.is_empty()
                    || d.view.selected_object.is_some()
                    || d.view.editing_board_object().is_some()
                    || d.has_unresolved_ink());
            let pending_board = d.has_unresolved_ink()
                || d.view.board_tool == BoardTool::Connector
                || ctx.input(|input| input.pointer.any_down() || input.any_touches());
            let guarded_shortcut = |action: &Action| {
                fim_hierarchy_shortcut(action)
                    || (pending_board && matches!(action, Action::Undo | Action::Redo))
            };
            // egui's logical shortcut matching ignores extra Alt/Shift. Do
            // not let a plain-arrow binding consume a guarded Alt+arrow
            // before the board controller or FIM command path sees it.
            let guarded_keys = if board_context {
                ctx.input(|input| {
                    input
                        .events
                        .iter()
                        .filter_map(|event| match event {
                            egui::Event::Key {
                                key,
                                modifiers,
                                pressed: true,
                                ..
                            } if shortcuts.iter().any(|(shortcut, action)| {
                                guarded_shortcut(action)
                                    && *key == shortcut.logical_key
                                    && modifiers.matches_logically(shortcut.modifiers)
                            }) =>
                            {
                                Some((*key, *modifiers))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
            } else {
                Vec::new()
            };
            let board_shortcuts: Vec<_> = if board_context {
                shortcuts
                    .iter()
                    .filter(|(shortcut, action)| {
                        !guarded_shortcut(action)
                            && !guarded_keys.iter().any(|(key, modifiers)| {
                                *key == shortcut.logical_key
                                    && modifiers.matches_logically(shortcut.modifiers)
                            })
                    })
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            let key_presses = if board_context {
                ctx.input(|input| {
                    input
                        .events
                        .iter()
                        .filter(|event| matches!(event, egui::Event::Key { pressed: true, .. }))
                        .count()
                })
            } else {
                0
            };
            // A mixed board/topic selection can still start a map-topic
            // drag. Route its release through FIM preview even though board
            // input normally needs the live sheet identity.
            let preview_input = fim
                && (!board_context || d.view.has_active_topic_drag())
                && ctx.input(|input| {
                    input.pointer.any_released()
                        || canvas_focused
                            && input.events.iter().any(|event| {
                                matches!(
                                    event,
                                    egui::Event::Key { pressed: true, .. }
                                        | egui::Event::Text(_)
                                        | egui::Event::Cut
                                        | egui::Event::Paste(_)
                                )
                            })
                });
            let mut preview = preview_input.then(|| d.doc.preview());
            let out = MapCanvas::show_with_shortcuts(
                ui,
                preview.as_mut().unwrap_or(&mut d.doc),
                &mut d.view,
                if board_context {
                    &board_shortcuts
                } else {
                    &shortcuts
                },
            );
            // Board shortcuts get first refusal (for example Alt+arrow
            // duplicates a selection). Forward only an unconsumed map
            // hierarchy shortcut, and reject it while a gesture is pending.
            let key_consumed = board_context
                && ctx.input(|input| {
                    input
                        .events
                        .iter()
                        .filter(|event| matches!(event, egui::Event::Key { pressed: true, .. }))
                        .count()
                        < key_presses
                });
            let deferred_shortcut = if board_context
                && !key_consumed
                && !out.minimap_blocks_keyboard
                && !egui::Popup::is_any_open(&ctx)
                && fim_canvas_owns_keyboard(&ctx, &d.view)
            {
                shortcuts
                    .iter()
                    .filter(|(_, action)| guarded_shortcut(action))
                    .find_map(|(shortcut, action)| {
                        ctx.input_mut(|input| input.consume_shortcut(shortcut))
                            .then(|| (action.clone(), pending_board))
                    })
            } else {
                None
            };
            d.record_ink_output(&out.ink_outcomes, &out.ink_errors);
            let clipboard_requests = out.clipboard_requests.clone();
            let clipboard_text = d.view.clipboard_out.take();
            (
                out,
                preview,
                clipboard_requests,
                clipboard_text,
                deferred_shortcut,
            )
        };
        for action in clipboard_requests {
            self.clipboard_action(&ctx, action);
        }
        if let Some(text) = clipboard_text {
            ctx.copy_text(text);
        }
        if let Some(link) = &out.open_link {
            self.open_link(link);
        }
        if let Some(error) = &out.error
            && !out
                .ink_errors
                .iter()
                .any(|ink| ink.to_string() == error.to_string())
        {
            let error = error.to_string();
            self.toasts.warning(format!(
                "{}: {}",
                t("Not possible here"),
                i18n::localize_model_error(&error)
            ));
        }
        if let Some(preview) = preview {
            self.accept_fim_rendered_preview(&ctx, preview);
        }
        if let Some((action, pending_board)) = deferred_shortcut {
            if pending_board {
                self.toasts.warning(t("Not possible here"));
            } else {
                self.exec(&ctx, Cmd::Canvas(action));
            }
        }
        if let Some(action) = &out.authoring_action {
            self.exec(&ctx, Cmd::Canvas(action.clone()));
            if let Some(text) = &out.authoring_text {
                self.set_fim_node_name(text);
            }
        }
        let mut cmd = None;
        out.context_menu(|ui| {
            scroll_menu(ui, |ui| {
                if let Some((id, index)) = self.doc().and_then(|d| d.view.context_marker) {
                    if let Some(command) = self.symbol_instance(ui, id, index, true) {
                        cmd = Some(command);
                    }
                } else {
                    self.context_menu(ui, &mut cmd);
                }
            });
        });
        if let Some(c) = cmd {
            self.exec(&ctx, c);
        }
    }

    pub(crate) fn has_card_text_target(&self) -> bool {
        self.card_text_target().is_some()
    }

    fn card_text_target(&self) -> Option<(CardTextTarget, bool)> {
        let doc = self.doc()?;
        let view = &doc.view;
        if view.selection.len() != 1
            || !view.board_selection.is_empty()
            || view.selected_object.is_some()
            || view.relationship_from.is_some()
            || view.editing_board_object().is_some()
        {
            return None;
        }
        let id = view.primary()?;
        if view.editing_topic().is_some_and(|editing| editing != id) {
            return None;
        }
        let sheet = doc.doc.sheet();
        let card = sheet.topic(id)?.card?;
        if !matches!(card.kind, CardKind::Sticky | CardKind::Text)
            || !sheet.is_visible(id)
            || sheet.board_member_for_topic(id).is_none()
        {
            return None;
        }
        let targets = StyleTargets::from_view(sheet, view)?;
        let enabled = !targets.locked
            && targets.topics == [id]
            && targets.shapes.is_empty()
            && !view.has_active_canvas_gesture()
            && !doc.has_inspector_changes()
            && doc.fim_edit.is_none()
            && view.title_draft().is_none_or(|(topic, original, _)| {
                sheet
                    .topic(topic)
                    .is_some_and(|item| item.title == original)
            })
            && doc.ensure_no_pending_ink().is_ok();
        Some((
            CardTextTarget {
                document: doc.key.clone(),
                instance: doc.doc.instance_id(),
                sheet: doc.doc.active_index(),
                root: sheet.root,
                topic: id,
                revision: doc.doc.state_id(),
            },
            enabled,
        ))
    }

    fn open_card_text_toolbar(ctx: &Context, viewport: egui::ViewportId, target: CardTextTarget) {
        let slot = card_text_slot(viewport);
        ctx.data_mut(|data| {
            data.insert_temp(slot, target);
            data.insert_temp(slot.with("focus"), true);
            data.insert_temp(slot.with("size_editing"), false);
        });
        ctx.request_repaint();
    }

    fn apply_card_text_change(
        &mut self,
        target: &CardTextTarget,
        change: CardTextChange,
    ) -> Option<CardTextTarget> {
        if !self
            .card_text_target()
            .is_some_and(|(current, enabled)| enabled && current == *target)
        {
            return None;
        }
        let doc = self.doc_mut()?;
        let result = doc.doc.edit_sheet(change.label(), |sheet| {
            sheet.update_style(&[target.topic], |style| change.apply(style))
        });
        match result {
            Ok(()) => Some(CardTextTarget {
                revision: doc.doc.state_id(),
                ..target.clone()
            }),
            Err(error) => {
                self.toasts.error(error.to_string());
                None
            }
        }
    }

    pub(crate) fn save_card_with_pending_input(&mut self, ctx: &Context, save_as: bool) -> bool {
        let viewport = self.current_viewport;
        let continuation = card_text_input_has_pending_events(ctx, viewport)
            .then(|| self.card_text_target())
            .flatten()
            .and_then(|(target, _)| {
                self.doc()
                    .and_then(|doc| doc.view.capture_title_edit_continuation())
                    .map(|edit| (target, edit))
            });
        let saved = self.save_active(save_as);
        if let Some((target, edit)) = continuation {
            let resumed = self.doc_mut().is_some_and(|doc| {
                doc.doc.instance_id() == target.instance
                    && doc.doc.active_index() == target.sheet
                    && doc.doc.sheet().root == target.root
                    && doc.view.resume_title_edit_after_commit(&doc.doc, edit)
            });
            if resumed && let Some((current, _)) = self.card_text_target() {
                let slot = card_text_slot(viewport);
                ctx.data_mut(|data| {
                    if data.get_temp::<CardTextTarget>(slot).as_ref() == Some(&target) {
                        data.insert_temp(slot, current);
                    }
                });
            }
        }
        saved
    }

    pub(crate) fn defer_card_text_command(&self, ctx: &Context, command: &Cmd) -> bool {
        let executing = card_text_slot(egui::ViewportId::ROOT)
            .with("deferred_operations")
            .with("executing");
        let is_executing = ctx
            .data(|data| data.get_temp::<bool>(executing))
            .unwrap_or(false);
        let slot = card_text_slot(self.current_viewport);
        let frame = ctx.cumulative_frame_nr();
        let before_editor = ctx.data(|data| data.get_temp::<u64>(slot.with("before_editor")));
        let after_editor = ctx.data(|data| data.get_temp::<u64>(slot.with("after_editor")));
        let in_first_pass = ctx.viewport_for(self.current_viewport, |state| {
            state.output.num_completed_passes == 0
        });
        if !is_executing
            && before_editor != Some(frame)
            && after_editor != Some(frame)
            && in_first_pass
            && (card_text_input_has_pending_events(ctx, self.current_viewport)
                || (card_text_input_in_scope(ctx, self.current_viewport)
                    && ctx.input(|input| input.events.iter().any(card_text_input_is_action))))
            && let Some(doc) = self.doc()
            && self.queue_card_text_operation(
                ctx,
                DeferredCardTextOperation {
                    origin: self.current_viewport,
                    not_before_root_frame: ctx
                        .cumulative_frame_nr_for(egui::ViewportId::ROOT)
                        .saturating_add(u64::from(matches!(command, Cmd::SaveAll))),
                    action: DeferredCardTextAction::Command {
                        instance: doc.doc.instance_id(),
                        sheet: doc.doc.active_index(),
                        root: doc.doc.sheet().root,
                        command: self.capture_card_text_command(command),
                    },
                },
                true,
            )
        {
            return true;
        }
        if matches!(command, Cmd::SaveAll)
            && !is_executing
            && self.queue_card_text_operation(
                ctx,
                DeferredCardTextOperation {
                    origin: self.current_viewport,
                    not_before_root_frame: ctx
                        .cumulative_frame_nr_for(egui::ViewportId::ROOT)
                        .saturating_add(1),
                    action: DeferredCardTextAction::SaveAll,
                },
                std::iter::once(egui::ViewportId::ROOT)
                    .chain(self.document_windows.values().copied())
                    .any(|viewport| {
                        viewport != self.current_viewport && card_text_input_in_scope(ctx, viewport)
                    }),
            )
        {
            return true;
        }
        if before_editor != Some(frame)
            || !ctx.text_edit_focused()
            || !ctx.input(|input| {
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Text(_)
                            | egui::Event::Paste(_)
                            | egui::Event::Cut
                            | egui::Event::Ime(_)
                            | egui::Event::Key {
                                key: Key::Backspace | Key::Delete,
                                pressed: true,
                                ..
                            }
                    )
                })
            })
        {
            return false;
        }
        let Some((target, _)) = self.card_text_target() else {
            return false;
        };
        let focused = ctx.memory(|memory| memory.focused());
        let title_editing = self
            .doc()
            .is_some_and(|doc| doc.view.editing_topic() == Some(target.topic))
            && focused == Some(MapCanvas::title_editor_id());
        let size_editing = ctx.data(|data| {
            data.get_temp::<bool>(slot.with("size_editing"))
                .unwrap_or(false)
                && focused.is_some()
                && data.get_temp::<egui::Id>(slot.with("size_focus")) == focused
        });
        if !title_editing && !size_editing {
            return false;
        }
        ctx.data_mut(|data| {
            let mut pending = data
                .get_temp::<Vec<DeferredCardTextCommand>>(slot.with("deferred_commands"))
                .unwrap_or_default();
            pending.push(DeferredCardTextCommand {
                instance: target.instance,
                sheet: target.sheet,
                root: target.root,
                command: self.capture_card_text_command(command),
            });
            data.insert_temp(slot.with("deferred_commands"), pending);
        });
        true
    }

    fn finish_deferred_card_text_commands(&mut self, ctx: &Context) {
        let slot = card_text_slot(self.current_viewport);
        let frame = ctx.cumulative_frame_nr();
        let pending = ctx.data_mut(|data| {
            data.remove::<u64>(slot.with("before_editor"));
            data.insert_temp(slot.with("after_editor"), frame);
            let pending = data
                .get_temp::<Vec<DeferredCardTextCommand>>(slot.with("deferred_commands"))
                .unwrap_or_default();
            data.remove::<Vec<DeferredCardTextCommand>>(slot.with("deferred_commands"));
            pending
        });
        for deferred in pending {
            if self.doc().is_some_and(|doc| {
                doc.doc.instance_id() == deferred.instance
                    && doc.doc.active_index() == deferred.sheet
                    && doc.doc.sheet().root == deferred.root
            }) && let Some(command) = self.resolve_card_text_command(deferred.command)
            {
                self.exec(ctx, command);
            } else {
                self.toasts.warning(t("Not possible here"));
            }
        }
    }

    fn finish_card_text_focus_handoff(&self, ctx: &Context) {
        let slot = card_text_slot(self.current_viewport);
        let pending = ctx.data_mut(|data| {
            let pending =
                data.get_temp::<CardTextFocusAfterEditor>(slot.with("focus_after_editor"));
            data.remove::<CardTextFocusAfterEditor>(slot.with("focus_after_editor"));
            pending
        });
        let Some(pending) = pending else { return };
        let same_edit = self.doc().is_some_and(|doc| {
            doc.view.editing_topic() == Some(pending.target.topic)
                && doc.view.title_edit_serial() == Some(pending.serial)
                && !doc.view.title_ime_active(ctx)
        });
        if pending.frame == ctx.cumulative_frame_nr()
            && same_edit
            && self
                .card_text_target()
                .is_some_and(|(target, enabled)| enabled && target == pending.target)
            && ctx.memory(|memory| memory.has_focus(MapCanvas::title_editor_id()))
            && !ctx.input(|input| input.pointer.any_pressed() || input.pointer.any_released())
        {
            ctx.memory_mut(|memory| memory.request_focus(pending.control));
            ctx.request_repaint();
        }
    }

    fn card_text_toolbar(&mut self, ui: &mut Ui) -> bool {
        let ctx = ui.ctx().clone();
        let viewport = self.current_viewport;
        let slot = card_text_slot(viewport);
        if let Some(restore) =
            ctx.data(|data| data.get_temp::<CardTitleRestore>(slot.with("restore_title")))
            && restore.frame < ctx.cumulative_frame_nr()
        {
            ctx.data_mut(|data| data.remove::<CardTitleRestore>(slot.with("restore_title")));
            let same_edit = self.doc().is_some_and(|doc| {
                doc.doc.instance_id() == restore.instance
                    && doc.doc.active_index() == restore.sheet
                    && doc.doc.sheet().root == restore.root
                    && doc.view.editing_topic() == Some(restore.topic)
                    && doc.view.title_edit_serial() == Some(restore.serial)
            });
            let focus = ctx.memory(|memory| memory.focused());
            let pointer_event =
                ctx.input(|input| input.pointer.any_pressed() || input.pointer.any_released());
            if same_edit
                && restore.frame.saturating_add(1) == ctx.cumulative_frame_nr()
                && !pointer_event
                && focus.is_none_or(|id| id == MapCanvas::title_editor_id())
            {
                ctx.memory_mut(|memory| memory.request_focus(MapCanvas::title_editor_id()));
                if let Some(doc) = self.doc_mut() {
                    doc.view.retain_title_draft_this_frame(&ctx, true);
                }
            }
        }
        if let Some((current, true)) = self.card_text_target()
            && let Some(serial) = self.doc().and_then(|doc| doc.view.title_edit_serial())
        {
            let edit = (
                current.instance,
                current.sheet,
                current.root,
                current.topic,
                serial,
            );
            let seen = ctx.data(|data| {
                data.get_temp::<(u64, usize, TopicId, TopicId, u64)>(slot.with("title_edit"))
            });
            if seen != Some(edit) {
                ctx.data_mut(|data| data.insert_temp(slot.with("title_edit"), edit));
                if ctx.data(|data| data.get_temp::<CardTextTarget>(slot).is_none()) {
                    Self::open_card_text_toolbar(&ctx, viewport, current);
                    ctx.data_mut(|data| data.insert_temp(slot.with("focus"), false));
                }
            }
        }
        let owned_before = ctx
            .data(|data| data.get_temp::<Vec<egui::Id>>(slot.with("owned_focus")))
            .unwrap_or_default();
        let focus = ctx.memory(|memory| memory.focused());
        let editor_focused = focus == Some(MapCanvas::title_editor_id());
        let toolbar_focused = focus.is_some_and(|id| owned_before.contains(&id))
            || (focus.is_none()
                && ctx.memory(|memory| {
                    owned_before
                        .iter()
                        .any(|id| memory.had_focus_last_frame(*id))
                }));
        let mut focus_toolbar_after_editor = None;
        let mut focus_editor_after_toolbar = false;
        if let Some((current, true)) = self.card_text_target()
            && self.doc().is_some_and(|doc| {
                doc.view.editing_topic() == Some(current.topic) && !doc.view.title_ime_active(&ctx)
            })
            && !egui::Popup::is_any_open(&ctx)
            && (editor_focused
                || (toolbar_focused
                    && !ctx.text_edit_focused()
                    && ctx
                        .data(|data| data.get_temp::<CardTextTarget>(slot))
                        .as_ref()
                        == Some(&current)))
            && ctx.input_mut(|input| input.consume_key(Modifiers::ALT, Key::F10))
        {
            if editor_focused {
                focus_toolbar_after_editor =
                    self.doc().and_then(|doc| doc.view.title_edit_serial());
                Self::open_card_text_toolbar(&ctx, viewport, current);
                ctx.data_mut(|data| data.insert_temp(slot.with("focus"), false));
            } else {
                focus_editor_after_toolbar = true;
            }
        }
        let Some(target) = ctx.data(|data| data.get_temp::<CardTextTarget>(slot)) else {
            return false;
        };
        let Some((current, mut enabled)) = self.card_text_target() else {
            ctx.data_mut(|data| data.remove::<CardTextTarget>(slot));
            return false;
        };
        if current != target {
            ctx.data_mut(|data| data.remove::<CardTextTarget>(slot));
            return false;
        }
        let title_active = self
            .doc()
            .is_some_and(|doc| doc.view.editing_topic() == Some(target.topic));
        let ime_active = title_active
            && self
                .doc()
                .is_some_and(|doc| doc.view.title_ime_active(&ctx));
        enabled &= !ime_active;
        let size_was_editing = ctx
            .data(|data| data.get_temp::<bool>(slot.with("size_editing")))
            .unwrap_or(false);
        let size_rect = ctx.data(|data| data.get_temp::<egui::Rect>(slot.with("size_rect")));
        let pending_size_input = size_was_editing
            && ctx.input(|input| {
                (input.pointer.any_pressed() || input.pointer.any_released())
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|pos| size_rect.is_some_and(|rect| !rect.contains(pos)))
                    && input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::Text(_) | egui::Event::Paste(_)))
            });
        let size_focus = ctx.data(|data| data.get_temp::<egui::Id>(slot.with("size_focus")));
        if enabled
            && !egui::Popup::is_any_open(&ctx)
            && !ctx.text_edit_focused()
            && !size_was_editing
            && (!title_active || toolbar_focused)
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            ctx.data_mut(|data| data.remove::<CardTextTarget>(slot));
            ctx.memory_mut(|memory| {
                memory.request_focus(if title_active {
                    MapCanvas::title_editor_id()
                } else {
                    MapCanvas::id()
                });
            });
            return false;
        }

        let doc = self.doc().unwrap();
        let sheet = doc.doc.sheet();
        let topic = sheet.topic(target.topic).unwrap();
        let style = topic.style.clone();
        let resolved = thinoda_core::theme::resolve_style(sheet, target.topic, doc.view.dark);
        let text_color = StyleTargets::from_view(sheet, &doc.view)
            .and_then(|targets| targets.common_color(sheet, doc.view.dark, StyleColor::Text))
            .unwrap_or(resolved.text_color);
        let numeric_owner = egui::Id::unique(("card_text_size", &target, doc.doc.state_id()));
        let request_focus = ctx.data_mut(|data| {
            let focus = data.get_temp::<bool>(slot.with("focus")).unwrap_or(false);
            data.remove::<bool>(slot.with("focus"));
            focus
        });
        let mut change = None;
        let mut size_change = None;
        let mut close = false;
        let mut command = None;
        let mut font_popup = None;
        let mut size_is_editing = false;
        let mut owned_focus = Vec::new();
        let mut bold_control = None;
        let popup_was_open = ctx
            .data(|data| data.get_temp::<bool>(slot.with("font_popup")))
            .unwrap_or(false);
        let palette = Palette::of(&ctx);
        let panel = egui::Panel::top(egui::Id::unique(("thinoda_card_text_toolbar", viewport)))
            .exact_size(48.0)
            .frame(
                egui::Frame::new()
                    .fill(palette.surface)
                    .inner_margin(Margin::symmetric(6, 3)),
            )
            .show(ui, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.push_id(
                        (
                            &target.document,
                            target.instance,
                            target.sheet,
                            target.root,
                            target.topic,
                        ),
                        |ui| {
                            ui.horizontal_centered(|ui| {
                                close = card_text_owned_icon(
                                    ui,
                                    icons::CLOSE,
                                    t("Close"),
                                    false,
                                    false,
                                    &mut owned_focus,
                                );
                                ui.add_enabled_ui(!title_active, |ui| {
                                    guarded_menu_button(
                                        ui,
                                        format!("{} {}", icons::ADD, icons::CHEVRON_DOWN),
                                        |ui| self.formatting_insert_actions(ui, &mut command),
                                    )
                                    .response
                                    .accessible_name(t("More insert actions"))
                                    .on_hover_text(t("More insert actions"));
                                });
                                let width = ui.available_width().max(0.0);
                                egui::ScrollArea::horizontal()
                                    .id_salt("card_text_controls")
                                    .scroll_source(egui::scroll_area::ScrollSource::ALL)
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                    )
                                    .max_width(width)
                                    .max_height(42.0)
                                    .min_scrolled_height(Tokens::ICON_BUTTON)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.horizontal_centered(|ui| {
                                            ui.label(RichText::new(t("Whole-card text")).small())
                                                .on_hover_text(format!(
                                                    "{} · Alt+F10",
                                                    card_text_label()
                                                ));
                                            for (icon, label, on, next, focus) in [
                                                (
                                                    icons::BOLD,
                                                    "Bold",
                                                    resolved.bold,
                                                    CardTextChange::Bold(!resolved.bold),
                                                    request_focus,
                                                ),
                                                (
                                                    icons::ITALIC,
                                                    "Italic",
                                                    resolved.italic,
                                                    CardTextChange::Italic(!resolved.italic),
                                                    false,
                                                ),
                                                (
                                                    icons::UNDERLINE,
                                                    "Underline",
                                                    resolved.underline,
                                                    CardTextChange::Underline(!resolved.underline),
                                                    false,
                                                ),
                                                (
                                                    icons::STRIKE,
                                                    "Strikethrough",
                                                    resolved.strikethrough,
                                                    CardTextChange::Strikethrough(
                                                        !resolved.strikethrough,
                                                    ),
                                                    false,
                                                ),
                                            ] {
                                                let activated = card_text_owned_icon(
                                                    ui,
                                                    icon,
                                                    t(label),
                                                    on,
                                                    focus,
                                                    &mut owned_focus,
                                                );
                                                if label == "Bold" {
                                                    bold_control = owned_focus.last().copied();
                                                }
                                                if activated {
                                                    change = Some(next);
                                                }
                                            }
                                            widgets::vdivider(ui);
                                            for (align, icon, label) in [
                                                (
                                                    TextAlign::Left,
                                                    icons::TEXT_ALIGN_LEFT,
                                                    "Align Left",
                                                ),
                                                (
                                                    TextAlign::Center,
                                                    icons::TEXT_ALIGN_CENTER,
                                                    "Align Center",
                                                ),
                                                (
                                                    TextAlign::Right,
                                                    icons::TEXT_ALIGN_RIGHT,
                                                    "Align Right",
                                                ),
                                            ] {
                                                if card_text_owned_icon(
                                                    ui,
                                                    icon,
                                                    t(label),
                                                    resolved.align == align,
                                                    false,
                                                    &mut owned_focus,
                                                ) && resolved.align != align
                                                {
                                                    change = Some(CardTextChange::Align(align));
                                                }
                                            }
                                            widgets::vdivider(ui);
                                            let mut family = style.font_family.clone();
                                            ui.allocate_ui_with_layout(
                                                vec2(120.0, Tokens::ICON_BUTTON),
                                                Layout::left_to_right(Align::Center),
                                                |ui| {
                                                    ui.set_max_width(120.0);
                                                    let response = font_choice(
                                                        ui,
                                                        &mut self.fonts,
                                                        &mut family,
                                                        t("Auto"),
                                                        true,
                                                        Some(&mut owned_focus),
                                                    )
                                                    .accessible_name(t("Font Family"));
                                                    font_popup = Some((
                                                        response.id,
                                                        egui::ComboBox::is_open(&ctx, response.id),
                                                    ));
                                                    if response.gained_focus() {
                                                        response.scroll_to_me(None);
                                                    }
                                                },
                                            );
                                            if family != style.font_family
                                                && widgets::touch_activation_allowed(&ctx)
                                            {
                                                change = Some(CardTextChange::Family(family));
                                            }
                                            let mut size = resolved.font_size;
                                            let previous_focus =
                                                ctx.memory(|memory| memory.focused());
                                            if pending_size_input
                                                && let Some(id) = size_focus
                                                && previous_focus != Some(id)
                                            {
                                                ctx.memory_mut(|memory| memory.request_focus(id));
                                            }
                                            let (response, commit) = touch_numeric_value(
                                                ui,
                                                &mut size,
                                                "card-text-size",
                                                Some(numeric_owner),
                                                |ui, value| {
                                                    let pointer = pending_size_input.then(|| {
                                                        ctx.input_mut(|input| {
                                                            std::mem::take(&mut input.pointer)
                                                        })
                                                    });
                                                    let response = ui
                                                        .add_sized(
                                                            vec2(62.0, Tokens::ICON_BUTTON),
                                                            egui::DragValue::new(value)
                                                                .range(6.0..=96.0)
                                                                .speed(0.5),
                                                        )
                                                        .accessible_name(t("Font Size"));
                                                    if let Some(pointer) = pointer {
                                                        ctx.input_mut(|input| {
                                                            input.pointer = pointer
                                                        });
                                                    }
                                                    response
                                                },
                                            );
                                            if pending_size_input
                                                && let Some(id) = size_focus
                                                && previous_focus != Some(id)
                                            {
                                                ctx.memory_mut(|memory| {
                                                    if let Some(previous) = previous_focus {
                                                        memory.request_focus(previous);
                                                    } else {
                                                        memory.surrender_focus(id);
                                                    }
                                                });
                                            }
                                            owned_focus.push(response.id);
                                            size_is_editing =
                                                response.has_focus() && ctx.text_edit_focused();
                                            let focused = ctx.memory(|memory| memory.focused());
                                            ctx.data_mut(|data| {
                                                data.insert_temp(
                                                    slot.with("size_editing"),
                                                    size_is_editing,
                                                );
                                                data.insert_temp(
                                                    slot.with("size_rect"),
                                                    response.rect,
                                                );
                                                if size_is_editing {
                                                    if let Some(focused) = focused {
                                                        data.insert_temp(
                                                            slot.with("size_focus"),
                                                            focused,
                                                        );
                                                    }
                                                } else {
                                                    data.remove::<egui::Id>(
                                                        slot.with("size_focus"),
                                                    );
                                                }
                                            });
                                            if response.gained_focus() {
                                                response.scroll_to_me(None);
                                            }
                                            if response.lost_focus() && enabled {
                                                // DragValue can surrender focus on commit/cancel before
                                                // the canvas handles its own keyboard shortcuts.
                                                ctx.input_mut(|input| {
                                                    input.consume_key(Modifiers::NONE, Key::Enter);
                                                    input.consume_key(Modifiers::NONE, Key::Escape);
                                                });
                                            }
                                            if commit
                                                && size != resolved.font_size
                                                && size.is_finite()
                                            {
                                                size_change = Some(CardTextChange::Size(size));
                                            }
                                            widgets::vdivider(ui);
                                            ui.label(t("Text Color"));
                                            let auto = ui
                                                .selectable_label(
                                                    style.text_color.is_none(),
                                                    t("Auto"),
                                                )
                                                .accessible_name(format!(
                                                    "{} · {}",
                                                    t("Text Color"),
                                                    t("Auto")
                                                ));
                                            owned_focus.push(auto.id);
                                            if auto.gained_focus() {
                                                auto.scroll_to_me(None);
                                            }
                                            if auto.clicked()
                                                && widgets::touch_activation_allowed(&ctx)
                                                && style.text_color.is_some()
                                            {
                                                change = Some(CardTextChange::Color(None));
                                            }
                                            let custom = (!SWATCHES
                                                .contains(&from_color(text_color)))
                                            .then_some(text_color);
                                            for color in custom
                                                .into_iter()
                                                .chain(SWATCHES.iter().copied().map(to_color))
                                            {
                                                if card_text_swatch(
                                                    ui,
                                                    color,
                                                    color == text_color,
                                                    &mut owned_focus,
                                                ) {
                                                    change =
                                                        Some(CardTextChange::Color(Some(color)));
                                                }
                                            }
                                        });
                                        reveal_changed_focus(ui);
                                    });
                            });
                        },
                    );
                });
            });
        let popup_open = font_popup.is_some_and(|(_, open)| open);
        let popup_rect =
            font_popup.and_then(|(id, _)| ctx.memory(|memory| memory.area_rect(id.with("popup"))));
        ctx.data_mut(|data| data.insert_temp(slot.with("font_popup"), popup_open));
        let (pointer_event, pointer_released) = ctx.input(|input| {
            let released = input.pointer.any_released();
            (
                (input.pointer.any_pressed() || released)
                    .then(|| input.pointer.interact_pos())
                    .flatten(),
                released,
            )
        });
        let panel_rect = panel.response.rect;
        let pointer_in_companion = pointer_event.is_some_and(|pos| {
            panel_rect.contains(pos)
                || ((popup_open || popup_was_open)
                    && popup_rect.is_some_and(|rect| rect.contains(pos)))
        });
        if title_active && ime_active && pointer_in_companion {
            ctx.memory_mut(|memory| memory.request_focus(MapCanvas::title_editor_id()));
        }
        ctx.data_mut(|data| data.insert_temp(slot.with("owned_focus"), owned_focus.clone()));
        let focus = ctx.memory(|memory| memory.focused());
        let focused_in_companion = focus.is_some_and(|id| owned_focus.contains(&id));
        let has_change = size_change.is_some() || change.is_some();
        let keyboard_finish = pointer_event.is_none()
            && ((size_was_editing && !size_is_editing && focus.is_none())
                || (has_change && (focus.is_none() || (popup_was_open && !popup_open))));
        let refocus = ime_active
            || keyboard_finish
            || (pointer_released && (has_change || close || (!popup_open && !size_is_editing)));
        if title_active
            && if pointer_event.is_some() {
                pointer_in_companion
            } else {
                focused_in_companion || (popup_open && focus.is_none()) || keyboard_finish
            }
            && let Some(doc) = self.doc_mut()
        {
            doc.view.retain_title_draft_this_frame(&ctx, refocus);
            if (pointer_released || keyboard_finish)
                && refocus
                && let Some(serial) = doc.view.title_edit_serial()
            {
                let frame = ctx.cumulative_frame_nr();
                ctx.data_mut(|data| {
                    data.insert_temp(
                        slot.with("restore_title"),
                        CardTitleRestore {
                            instance: target.instance,
                            sheet: target.sheet,
                            root: target.root,
                            topic: target.topic,
                            serial,
                            frame,
                        },
                    );
                });
                ctx.request_repaint();
            }
        }
        let mut updated_target = target.clone();
        let mut changed = false;
        for change in size_change.into_iter().chain(change) {
            let Some(updated) = self.apply_card_text_change(&updated_target, change) else {
                break;
            };
            updated_target = updated;
            changed = true;
        }
        if close {
            ctx.data_mut(|data| data.remove::<CardTextTarget>(slot));
            ctx.memory_mut(|memory| {
                memory.request_focus(if title_active {
                    MapCanvas::title_editor_id()
                } else {
                    MapCanvas::id()
                });
            });
        } else if changed {
            ctx.data_mut(|data| data.insert_temp(slot, updated_target.clone()));
            if title_active && ctx.input(|input| input.pointer.any_released()) {
                ctx.memory_mut(|memory| memory.request_focus(MapCanvas::title_editor_id()));
            }
        }
        if focus_editor_after_toolbar && title_active && !close {
            ctx.memory_mut(|memory| memory.request_focus(MapCanvas::title_editor_id()));
            if let Some(doc) = self.doc_mut() {
                doc.view.retain_title_draft_this_frame(&ctx, true);
            }
        }
        if let Some(serial) = focus_toolbar_after_editor
            && let Some(control) = bold_control
            && !close
        {
            let frame = ctx.cumulative_frame_nr();
            ctx.data_mut(|data| {
                data.insert_temp(
                    slot.with("focus_after_editor"),
                    CardTextFocusAfterEditor {
                        target: updated_target,
                        serial,
                        control,
                        frame,
                    },
                );
            });
        }
        if let Some(command) = command {
            self.exec(&ctx, command);
        }
        !close
    }

    fn topic_toolbar(
        &mut self,
        ctx: &Context,
        board_toolbar: Option<egui::Rect>,
        editing_toolbar: Option<egui::Rect>,
    ) {
        if !self.settings.show_topic_toolbar {
            return;
        }
        let viewport = self.current_viewport;
        let text_target = self.card_text_target();
        let text_open = text_target.as_ref().is_some_and(|(target, _)| {
            ctx.data(|data| data.get_temp::<CardTextTarget>(card_text_slot(viewport)))
                .as_ref()
                == Some(target)
        });
        let ai_enabled = self.settings.ai.enabled;
        let Some(d) = self.doc_mut() else { return };
        if d.view.selection.len() != 1
            || d.view.editing_topic().is_some()
            || d.view.editing_board_object().is_some()
            || d.view.relationship_from.is_some()
        {
            return;
        }
        let Some(unrotated_rect) = d.view.selected_topic_rect(&d.doc) else {
            return;
        };
        let Some(selected) = d.view.primary() else {
            return;
        };
        let canvas = d.view.canvas_rect();
        let topic = d.doc.sheet().topic(selected);
        let has_link = topic.is_some_and(|topic| topic.hyperlink.is_some());
        let todo = topic.is_some_and(|topic| topic.todo.is_some());
        let card = topic.and_then(|topic| topic.card);
        let is_board = d.doc.sheet().uses_board_semantics();
        let origin = canvas.center() + d.view.offset;
        let zoom = d.view.zoom;
        let layout = is_board.then(|| d.view.layout_for(&d.doc));
        let rect = layout
            .as_ref()
            .and_then(|layout| layout.rect(selected))
            .map(|rect| {
                let rotation = card.map_or(0.0, |card| card.rotation);
                board_screen_rect(
                    thinoda_core::board::rotated_bounds(rect, rotation),
                    origin,
                    zoom,
                )
            })
            .unwrap_or(unrotated_rect);
        let below = rect.top() - 52.0 < canvas.top();
        let mut position = egui::pos2(
            rect.center().x,
            if below {
                rect.bottom() + 12.0
            } else {
                rect.top() - 12.0
            },
        );
        let toolbar_id = egui::Id::unique("thinoda_topic_toolbar");
        let mut available = canvas.shrink(4.0);
        let mut safe_clip = None;
        if is_board && !available.is_positive() {
            return;
        }
        let previous_size = ctx
            .memory(|memory| memory.area_rect(toolbar_id))
            .map(|rect| rect.size());
        let measure_id = toolbar_id.with(("board_measure_key", ctx.viewport_id()));
        let measure_key = is_board.then(|| {
            topic_toolbar_measure_key(
                ctx,
                available,
                &d.key,
                ai_enabled,
                has_link,
                todo,
                text_target.is_some(),
            )
        });
        if !is_board {
            ctx.data_mut(|data| data.remove::<TopicToolbarMeasureKey>(measure_id));
        }
        let sizing_pass = is_board
            && (previous_size.is_none()
                || ctx
                    .data(|data| data.get_temp::<TopicToolbarMeasureKey>(measure_id))
                    .as_ref()
                    != measure_key.as_ref());
        if board_toolbar.is_some() && previous_size.is_none() {
            ctx.request_repaint();
        }
        if is_board && !sizing_pass {
            let Some(size) = previous_size else { return };
            if size.x > available.width() || size.y > available.height() {
                return;
            }
            let left =
                (position.x - size.x * 0.5).clamp(available.left(), available.right() - size.x);
            let top = (if below {
                position.y
            } else {
                position.y - size.y
            })
            .clamp(available.top(), available.bottom() - size.y);
            let desired = egui::Rect::from_min_size(egui::pos2(left, top), size);
            let Some(layout) = layout.as_ref() else {
                return;
            };
            let sheet = d.doc.sheet();
            let mut obstacles = board_card_obstacles(sheet, layout, origin, zoom, canvas);
            if card.is_none() {
                let mut visible = rect;
                if let Some(clip) = thinoda_core::scene::topic_clip_rect(sheet, selected) {
                    visible = visible.intersect(board_screen_rect(clip, origin, zoom));
                }
                visible = visible.intersect(canvas);
                if visible.is_positive() {
                    obstacles.push(visible);
                }
            }
            for zone in sheet.zones.iter().filter(|zone| !zone.name.is_empty()) {
                let frame = board_screen_rect(zone.frame, origin, zoom);
                if !frame.intersects(canvas) {
                    continue;
                }
                // Frame names may contain newlines. Match the fixed-size text
                // used by the canvas rather than assuming a single title row.
                let title_height = ctx.fonts_mut(|fonts| {
                    let galley = fonts.layout_no_wrap(
                        zone.name.clone(),
                        egui::FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                    galley.size().y.max(galley.mesh_bounds.bottom())
                });
                let title = egui::Rect::from_min_max(
                    frame.min,
                    egui::pos2(
                        frame.right(),
                        (frame.top() + (6.0 + title_height).max(24.0)).min(frame.bottom()),
                    ),
                )
                .intersect(canvas);
                if title.is_positive() {
                    obstacles.push(title);
                }
            }
            if let Some(board) = board_toolbar {
                obstacles.push(board.expand(6.0));
                if let Some(editing) = editing_toolbar {
                    obstacles.push(editing.expand(4.0));
                }
            }
            // Allow for card outlines as well as egui's Area rounding.
            let pixels_per_point = ctx.pixels_per_point();
            if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
                return;
            }
            let clearance = 2.0 / pixels_per_point + 1.125;
            if !clearance.is_finite() {
                return;
            }
            let Some(clear) = nearest_clear_toolbar_rect(desired, available, &obstacles, clearance)
            else {
                return;
            };
            // Preserve the card frame's shadow where it fits; do not move the
            // toolbar farther away merely to make room for the shadow.
            let shadow_clip = clear.expand(8.0 + clearance * 0.5);
            // If egui unexpectedly changes the size this pass, neither its
            // paint nor its widgets may escape the last safe placement.
            safe_clip = Some(
                if obstacles
                    .iter()
                    .all(|obstacle| !shadow_clip.intersects(obstacle.expand(clearance * 0.5)))
                {
                    shadow_clip
                } else {
                    clear.expand(clearance * 0.5)
                },
            );
            position = egui::pos2(
                clear.center().x,
                if below { clear.top() } else { clear.bottom() },
            );
        } else if !is_board && let (Some(board), Some(size)) = (board_toolbar, previous_size) {
            // Keep the existing non-board toolbar placement unchanged.
            let left = (position.x - size.x * 0.5).clamp(
                available.left(),
                (available.right() - size.x).max(available.left()),
            );
            let top = (if below {
                position.y
            } else {
                position.y - size.y
            })
            .clamp(
                available.top(),
                (available.bottom() - size.y).max(available.top()),
            );
            let desired = egui::Rect::from_min_size(egui::pos2(left, top), size);
            if board.expand(6.0).intersects(desired) {
                let left = board.right() + 8.0;
                if available.right() - left < size.x {
                    return;
                }
                available.min.x = left;
            }
        }
        let mut cmd = None;
        let mut toggle_text = false;
        let shown = egui::Area::new(toolbar_id)
            .order(egui::Order::Middle)
            .pivot(if below {
                egui::Align2::CENTER_TOP
            } else {
                egui::Align2::CENTER_BOTTOM
            })
            .fixed_pos(position)
            .constrain_to(available)
            .movable(false)
            .sizing_pass(sizing_pass)
            .show(ctx, |ui| {
                if let Some(clip) = safe_clip {
                    ui.shrink_clip_rect(clip);
                }
                widgets::card(ui).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if self.settings.ai.enabled {
                            if ui
                                .add_enabled(
                                    self.command_available(&Cmd::Ai(
                                        crate::ai::AiCommand::Brainstorm,
                                    )),
                                    egui::Button::new(format!(
                                        "{} {}",
                                        icons::BRAINSTORM,
                                        t("Expand ideas")
                                    )),
                                )
                                .clicked()
                            {
                                cmd = Some(Cmd::Ai(crate::ai::AiCommand::Brainstorm));
                            }
                            widgets::vdivider(ui);
                        }
                        for (icon, label, selected, command) in [
                            (
                                icons::SUBTOPIC,
                                "Add subtopic",
                                false,
                                Cmd::Canvas(Action::InsertChild),
                            ),
                            (
                                icons::RELATIONSHIP,
                                "Connect topics",
                                false,
                                Cmd::Canvas(Action::StartRelationship),
                            ),
                            (
                                icons::LINK,
                                "Edit hyperlink",
                                has_link,
                                Cmd::EditField(FocusField::Hyperlink),
                            ),
                            (
                                icons::NOTE,
                                "Edit notes",
                                false,
                                Cmd::EditField(FocusField::Notes),
                            ),
                            (icons::LABEL, "Edit labels", false, Cmd::Labels),
                            (
                                icons::TODO,
                                "Toggle to-do",
                                todo,
                                Cmd::Canvas(Action::ToggleTodo),
                            ),
                            (icons::FORMAT, "Topic style", false, Cmd::TopicStyle),
                        ] {
                            if widgets::icon_button(ui, icon, t(label), selected).clicked() {
                                cmd = Some(command);
                            }
                        }
                        if let Some((_, enabled)) = &text_target {
                            toggle_text = ui
                                .add_enabled_ui(*enabled, |ui| {
                                    card_text_icon(
                                        ui,
                                        icons::BOARD_TEXT,
                                        card_text_label(),
                                        text_open,
                                        false,
                                    )
                                })
                                .inner;
                        }
                    });
                });
            });
        if let Some(measure_key) = measure_key {
            ctx.data_mut(|data| data.insert_temp(measure_id, measure_key));
            if !sizing_pass
                && previous_size.is_some_and(|size| {
                    let actual = shown.response.rect.size();
                    (actual.x - size.x).abs() > 0.125 || (actual.y - size.y).abs() > 0.125
                })
            {
                if let Some(mut state) = egui::containers::AreaState::load(ctx, toolbar_id) {
                    state.interactable = false;
                    ctx.memory_mut(|memory| {
                        memory
                            .areas_mut()
                            .set_state(egui::LayerId::new(egui::Order::Middle, toolbar_id), state);
                    });
                }
                ctx.request_discard("board topic toolbar size changed");
                ctx.request_repaint();
                return;
            }
        }
        if let Some(command) = cmd {
            self.exec(ctx, command);
        }
        if toggle_text && let Some((target, true)) = text_target {
            if text_open {
                ctx.data_mut(|data| data.remove::<CardTextTarget>(card_text_slot(viewport)));
            } else {
                Self::open_card_text_toolbar(ctx, viewport, target);
            }
        }
    }

    fn context_menu(&mut self, ui: &mut Ui, cmd: &mut Option<Cmd>) {
        let (context, entries) = self.canvas_context_menu();
        let mut card_text = false;
        self.context_entries(ui, &entries, cmd, &mut card_text);
        let key = if card_text {
            Some(crate::menu_usage::card_text_key(context))
        } else {
            cmd.as_ref()
                .and_then(|command| crate::menu_usage::key(context, command))
        };
        if let Some(key) = key {
            self.record_menu_usage(&key);
        }
    }

    /// Renders context-menu rows; enablement always comes from `command_available`.
    fn context_entries(
        &self,
        ui: &mut Ui,
        entries: &[CtxEntry],
        cmd: &mut Option<Cmd>,
        card_text: &mut bool,
    ) {
        for entry in entries {
            match entry {
                CtxEntry::Separator => {
                    ui.separator();
                }
                CtxEntry::Caption(text) => {
                    widgets::caption(ui, t(text));
                }
                CtxEntry::Submenu {
                    label,
                    icon,
                    entries,
                } => {
                    let text = t(label);
                    let title = match icon {
                        Some(icon) => format!("{icon} {text}"),
                        None => text.to_owned(),
                    };
                    menu_button(ui, title, |ui| {
                        self.context_entries(ui, entries, cmd, card_text);
                    })
                    .response
                    .accessible_name(text);
                }
                CtxEntry::Command {
                    label,
                    command,
                    checked,
                } => {
                    let label = t(label);
                    let enabled = self.command_available(command);
                    let clicked = if let Some(checked) = checked {
                        let mut value = *checked;
                        ui.add_enabled(enabled, egui::Checkbox::new(&mut value, label))
                            .clicked()
                    } else {
                        let shortcut = Self::shortcut_hint_for_mode(ui.ctx(), command, self.mode);
                        let icon = crate::command_icons::command_glyph(command);
                        ui.add_enabled_ui(enabled, |ui| {
                            thinoda_ui::ribbon::menu_item(ui, Some(icon), label, shortcut)
                        })
                        .inner
                        .accessible_name(label)
                        .clicked()
                    };
                    if clicked {
                        *cmd = Some(command.clone());
                        ui.close();
                    }
                }
                CtxEntry::CardText => {
                    let Some((target, enabled)) = self.card_text_target() else {
                        continue;
                    };
                    let label = card_text_label();
                    if ui
                        .add_enabled_ui(enabled, |ui| {
                            thinoda_ui::ribbon::menu_item(
                                ui,
                                Some(icons::BOARD_TEXT.into()),
                                label,
                                None,
                            )
                        })
                        .inner
                        .accessible_name(label)
                        .clicked()
                        && widgets::touch_activation_allowed(ui.ctx())
                    {
                        Self::open_card_text_toolbar(ui.ctx(), self.current_viewport, target);
                        *card_text = true;
                        ui.close();
                    }
                }
            }
        }
    }

    /// Counts one context-menu activation when the user opted in; never reorders menus.
    fn record_menu_usage(&mut self, key: &str) {
        if !self.settings.collect_menu_usage {
            return;
        }
        let Some(paths) = self.services.paths.as_ref() else {
            return;
        };
        if let Err(error) = crate::menu_usage::record(paths, key) {
            self.toasts.warning(format!(
                "{}: {error}",
                t("Could not update menu usage counts")
            ));
        }
    }

    /// Opt-in, local-only context-menu counts: toggle, review and clear.
    fn menu_usage_section(&mut self, ui: &mut Ui, activation_allowed: bool) {
        // Counts need a local config directory, which the web app does not have.
        let Some(paths) = self.services.paths.clone() else {
            return;
        };
        widgets::section(ui, t("Menu usage"), true, |ui| {
            widgets::property_field(ui, t("Count context-menu use"), |ui| {
                widgets::toggle(
                    ui,
                    &mut self.settings.collect_menu_usage,
                    t("Count context-menu use"),
                )
            });
            widgets::caption(
                ui,
                t(
                    "Optional. Counts stay on this device, are never uploaded and never reorder menus.",
                ),
            );
            let shown_id = egui::Id::unique("thinoda_menu_usage_counts");
            let confirm_id = egui::Id::unique("thinoda_menu_usage_confirm_clear");
            let mut shown = ui.data(|d| d.get_temp::<Vec<(String, u32)>>(shown_id));
            let mut confirm = ui.data(|d| d.get_temp::<bool>(confirm_id).unwrap_or(false));
            ui.horizontal_wrapped(|ui| {
                let toggle = if shown.is_some() {
                    t("Hide counts")
                } else {
                    t("Show counts")
                };
                if ui.button(toggle).clicked() && activation_allowed {
                    shown = if shown.is_some() {
                        None
                    } else {
                        match crate::menu_usage::load(&paths) {
                            Ok(counts) => Some(crate::menu_usage::ranked(&counts)),
                            Err(error) => {
                                self.toasts.warning(format!(
                                    "{}: {error}",
                                    t("Could not read menu usage counts")
                                ));
                                None
                            }
                        }
                    };
                }
                if !confirm {
                    if ui.button(t("Clear counts")).clicked() && activation_allowed {
                        confirm = true;
                    }
                } else {
                    ui.label(t("Clear all menu usage counts?"));
                    if ui.button(t("Clear")).clicked() && activation_allowed {
                        confirm = false;
                        match crate::menu_usage::clear(&paths) {
                            Ok(()) => {
                                if let Some(counts) = shown.as_mut() {
                                    counts.clear();
                                }
                            }
                            Err(error) => {
                                self.toasts.warning(format!(
                                    "{}: {error}",
                                    t("Could not clear menu usage counts")
                                ));
                            }
                        }
                    }
                    if ui.button(t("Cancel")).clicked() && activation_allowed {
                        confirm = false;
                    }
                }
            });
            if let Some(counts) = &shown {
                if counts.is_empty() {
                    widgets::caption(ui, t("No counts recorded yet."));
                } else {
                    egui::Grid::new("menu_usage_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            for (key, count) in counts {
                                ui.label(key);
                                ui.label(count.to_string());
                                ui.end_row();
                            }
                        });
                }
            }
            ui.data_mut(|d| {
                match shown {
                    Some(counts) => {
                        d.insert_temp(shown_id, counts);
                    }
                    None => d.remove::<Vec<(String, u32)>>(shown_id),
                }
                d.insert_temp(confirm_id, confirm);
            });
        });
    }

    fn home(&mut self, ui: &mut Ui) {
        let mut cmd = None;
        let p = Palette::of_ui(ui);
        egui::Frame::new()
            .fill(p.bg)
            .inner_margin(Margin::same(32))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                ui.heading(RichText::new(t("Create a new map")).size(22.0).strong());
                let previous = self.template_category;
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt("template_category")
                        .selected_text(
                            self.template_category
                                .map_or(t("All"), |category| t(category.label())),
                        )
                        .width(ui.available_width().min(360.0))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.template_category, None, t("All"));
                            for category in templates::TemplateCategory::ALL {
                                ui.selectable_value(
                                    &mut self.template_category,
                                    Some(category),
                                    t(category.label()),
                                );
                            }
                        })
                        .response
                        .accessible_name(t("Template Category"));
                    if self.template_category.is_some() && ui.button(t("All templates")).clicked() {
                        self.template_category = None;
                    }
                });
                if previous != self.template_category {
                    self.template_choice = None;
                }
                ui.add_space(12.0);
                egui::Panel::bottom("template_actions")
                    .exact_size(64.0)
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.separator();
                        let side = ui.spacing().interact_size.y;
                        egui::Panel::right("home_github_badge")
                            .exact_size(side)
                            .resizable(false)
                            .frame(egui::Frame::NONE)
                            .show(ui, |ui| {
                                crate::brand::github_badge(ui, side);
                            });
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    self.template_choice.is_some(),
                                    egui::Button::new(t("Create")),
                                )
                                .clicked()
                            {
                                cmd = self.template_choice.map(Cmd::NewFromTemplate);
                            }
                            if ui.button(t("Cancel")).clicked() {
                                self.template_choice = None;
                                if let Some(index) = self.home_return_to.as_ref().and_then(|key| {
                                    self.docs.iter().position(|doc| &doc.key == key)
                                }) {
                                    self.active = index;
                                }
                            }
                            if let Some(template) = self.template_choice.and_then(|id| {
                                templates::TEMPLATES
                                    .iter()
                                    .find(|template| template.id == id)
                            }) {
                                ui.label(t(template.name));
                            } else {
                                widgets::caption(ui, t("Select a template, then choose Create."));
                            }
                        });
                    });
                egui::ScrollArea::vertical()
                    .id_salt((
                        "home_templates",
                        self.template_category
                            .map_or("all", |category| category.label()),
                    ))
                    .show(ui, |ui| {
                        if let Some(action) = crate::template_gallery::show(
                            ui,
                            self.template_category,
                            &mut self.template_choice,
                            &self.settings.recent_templates,
                        ) {
                            match action {
                                crate::template_gallery::GalleryAction::Navigate(category) => {
                                    self.template_category = Some(category);
                                    self.template_choice = None;
                                }
                                crate::template_gallery::GalleryAction::Create(id) => {
                                    cmd = Some(Cmd::NewFromTemplate(id));
                                }
                            }
                        }
                        ui.add_space(24.0);
                        ui.horizontal(|ui| {
                            ui.heading(RichText::new(t("Recent")).size(18.0).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if widgets::secondary_button(ui, t("Open File…")).clicked() {
                                    cmd = Some(Cmd::Open);
                                }
                            });
                        });
                        ui.add_space(6.0);
                        let recent: Vec<_> = self
                            .settings
                            .existing_recent()
                            .into_iter()
                            .cloned()
                            .collect();
                        if recent.is_empty() {
                            widgets::caption(ui, t("No recent files"));
                        }
                        for r in recent.iter().take(20) {
                            ui.horizontal(|ui| {
                                let pin_label = if r.pinned { t("Unpin") } else { t("Pin") };
                                if ui
                                    .small_button(if r.pinned { icons::PIN } else { icons::UNPIN })
                                    .accessible_name(pin_label)
                                    .on_hover_text(pin_label)
                                    .clicked()
                                {
                                    self.settings.pin_recent(&r.path, !r.pinned);
                                    self.save_settings();
                                }
                                let name = r
                                    .path
                                    .file_name()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                if ui.link(name).clicked() {
                                    cmd = Some(Cmd::OpenPath(r.path.clone()));
                                }
                                let cloud = thinoda_platform::cloud::provider_for_path(
                                    &self.services.cloud,
                                    &r.path,
                                );
                                if let Some(c) = cloud {
                                    widgets::chip(
                                        ui,
                                        &format!("{} {}", icons::CLOUD, c.name),
                                        p.accent,
                                    );
                                }
                                widgets::caption(
                                    ui,
                                    r.path
                                        .parent()
                                        .map(|x| x.display().to_string())
                                        .unwrap_or_default(),
                                );
                            });
                        }
                    });
            });
        if let Some(c) = cmd {
            let ctx = ui.ctx().clone();
            self.exec(&ctx, c);
        }
    }

    fn outliner(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut fim_record = None;
        let mut fim_node_request = None;
        if let Some((record, can_insert)) = self.doc().and_then(|doc| {
            let source = doc.doc.sheet().fim.as_ref()?;
            let record = source.record_for_topic(doc.view.primary()?)?;
            Some((record, source.node_parent(record).is_some()))
        }) {
            ui.horizontal_wrapped(|ui| {
                if ui.button(t("Edit FIM node")).clicked() {
                    fim_node_request = Some((record, None));
                }
                if ui.button(t("Add FIM child")).clicked() {
                    fim_node_request = Some((record, Some(crate::fim::FimInsertion::Child)));
                }
                if ui
                    .add_enabled(can_insert, egui::Button::new(t("Insert FIM node after")))
                    .clicked()
                {
                    fim_node_request = Some((record, Some(crate::fim::FimInsertion::After)));
                }
            });
        }
        let shortcuts = self.active_canvas_bindings(ctx.os());
        let touch_actions_allowed = widgets::touch_activation_allowed(&ctx);
        let p = Palette::of_ui(ui);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        let outline_id = egui::Id::unique((
            "outline_topics",
            &d.key,
            d.doc.workbook().active_sheet,
            d.doc.sheet().root,
        ));
        let focused = ctx.memory(|memory| memory.focused());
        let mut topic_focus = false;
        let frame = ctx.cumulative_frame_nr();
        let entering = ui.data_mut(|data| {
            let previous = data.get_temp::<u64>(outline_id.with("last_frame"));
            data.insert_temp(outline_id.with("last_frame"), frame);
            previous.is_none_or(|previous| previous.saturating_add(1) < frame)
        });
        let mut focus_topic = ui
            .data_mut(|data| data.remove_temp::<Option<TopicId>>(outline_id.with("focus_topic")))
            .flatten();
        if entering && !self.search.open {
            focus_topic = d.view.primary();
        }
        let primary = d.view.primary();
        let reveal_primary = ui.data_mut(|data| {
            let previous = data.get_temp::<Option<TopicId>>(outline_id.with("primary"));
            data.insert_temp(outline_id.with("primary"), primary);
            previous.is_some_and(|previous| previous != primary)
        });
        let mut action: Option<Action> = None;
        let selected_title = d
            .view
            .primary()
            .and_then(|id| d.doc.sheet().topic(id))
            .map(|topic| topic.title.clone());
        let detail = if ui.available_width() >= 1160.0 {
            thinoda_ui::ribbon::Detail::Large
        } else if ui.available_width() >= 880.0 {
            thinoda_ui::ribbon::Detail::Labeled
        } else {
            thinoda_ui::ribbon::Detail::Icon
        };
        let detail = if self.settings.compact_ribbon {
            detail.compact()
        } else {
            detail
        };
        let mut action_buttons = |ui: &mut Ui, buttons: &[(&'static str, Action)]| {
            for (label, command) in buttons {
                let enabled = command.available(&d.doc, &d.view);
                let (response, activated) = ui
                    .add_enabled_ui(enabled, |ui| {
                        thinoda_ui::ribbon::command_button(
                            ui,
                            crate::command_icons::command_glyph(&Cmd::Canvas(command.clone())),
                            t(label),
                            detail,
                            false,
                            None,
                        )
                    })
                    .inner;
                if response.gained_focus() {
                    response.scroll_to_me(None);
                }
                if touch_actions_allowed && activated {
                    response.request_focus();
                    action = Some(command.clone());
                }
            }
        };
        let header_height = ui.available_height().max(0.0) * 0.4;
        let group_columns = if ui.available_width() >= 500.0 {
            4
        } else if ui.available_width() >= 220.0 {
            2
        } else {
            1
        };
        egui::ScrollArea::vertical()
            .id_salt(outline_id.with("actions"))
            .min_scrolled_height(0.0)
            .max_height(header_height)
            .show(ui, |ui| {
                let summary = selected_title.as_ref().map_or_else(
                    || t("Select a topic to edit").to_owned(),
                    |title| format!("{}: {title}", t("Selected Topic")),
                );
                ui.add(egui::Label::new(&summary).truncate())
                    .on_hover_text(&summary);
                egui::Frame::new()
                    .fill(p.surface)
                    .inner_margin(Margin::symmetric(6, 4))
                    .show(ui, |ui| {
                        egui::Grid::new(outline_id.with("ribbon_groups"))
                            .num_columns(group_columns)
                            .min_col_width(0.0)
                            .spacing(egui::vec2(16.0, 8.0))
                            .show(ui, |ui| {
                                for (index, (caption, buttons)) in [
                                    (
                                        "Insert",
                                        &[
                                            ("Add Sibling", Action::InsertSibling),
                                            ("Add Child", Action::InsertChild),
                                        ][..],
                                    ),
                                    ("Edit", &[("Edit Topic", Action::StartEdit)][..]),
                                    (
                                        "Arrange",
                                        &[
                                            ("Move Up", Action::MoveUp),
                                            ("Move Down", Action::MoveDown),
                                            ("Indent", Action::Indent),
                                            ("Outdent", Action::Outdent),
                                        ][..],
                                    ),
                                    (
                                        "Delete",
                                        &[
                                            ("Delete Topic", Action::Delete),
                                            ("Delete Topic Only", Action::DeleteKeepChildren),
                                        ][..],
                                    ),
                                ]
                                .into_iter()
                                .enumerate()
                                {
                                    ui.vertical(|ui| {
                                        let (width, height) = match detail {
                                            thinoda_ui::ribbon::Detail::Large => {
                                                (buttons.len() as f32 * 120.0 - 8.0, 96.0)
                                            }
                                            thinoda_ui::ribbon::Detail::Labeled => {
                                                (204.0, 176.0 + ui.spacing().item_spacing.y * 3.0)
                                            }
                                            thinoda_ui::ribbon::Detail::Icon => {
                                                if group_columns == 4 {
                                                    (buttons.len() as f32 * 52.0 - 8.0, 44.0)
                                                } else {
                                                    (buttons.len().min(2) as f32 * 52.0 - 8.0, 96.0)
                                                }
                                            }
                                            thinoda_ui::ribbon::Detail::CompactLarge => {
                                                (buttons.len() as f32 * 108.0 - 4.0, 72.0)
                                            }
                                            thinoda_ui::ribbon::Detail::CompactLabeled => {
                                                (184.0, 136.0 + ui.spacing().item_spacing.y * 3.0)
                                            }
                                            thinoda_ui::ribbon::Detail::CompactIcon => {
                                                if group_columns == 4 {
                                                    (buttons.len() as f32 * 38.0 - 4.0, 34.0)
                                                } else {
                                                    (buttons.len().min(2) as f32 * 38.0 - 4.0, 72.0)
                                                }
                                            }
                                        };
                                        let labeled = matches!(
                                            detail,
                                            thinoda_ui::ribbon::Detail::Labeled
                                                | thinoda_ui::ribbon::Detail::CompactLabeled
                                        );
                                        let icon = matches!(
                                            detail,
                                            thinoda_ui::ribbon::Detail::Icon
                                                | thinoda_ui::ribbon::Detail::CompactIcon
                                        );
                                        ui.set_width(width);
                                        ui.allocate_ui(egui::vec2(width, height), |ui| {
                                            ui.set_min_height(height);
                                            if labeled {
                                                ui.vertical(|ui| action_buttons(ui, buttons));
                                            } else if icon && group_columns < 4 {
                                                ui.vertical(|ui| {
                                                    for chunk in buttons.chunks(2) {
                                                        ui.horizontal(|ui| {
                                                            action_buttons(ui, chunk)
                                                        });
                                                    }
                                                });
                                            } else {
                                                ui.horizontal(|ui| action_buttons(ui, buttons));
                                            }
                                        });
                                        ui.add_sized(
                                            [width, 16.0],
                                            egui::Label::new(
                                                egui::RichText::new(t(caption))
                                                    .size(10.0)
                                                    .color(p.text_muted),
                                            )
                                            .truncate(),
                                        );
                                    });
                                    if (index + 1) % group_columns == 0 {
                                        ui.end_row();
                                    }
                                }
                            })
                    });
            });
        let margin = Margin::symmetric(
            (ui.available_width().max(0.0) * 0.08).min(48.0) as i8,
            (ui.available_height().max(0.0) * 0.1).min(24.0) as i8,
        );
        egui::Frame::new()
            .fill(p.bg)
            .inner_margin(margin)
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                let edit_width = ui.available_width().clamp(120.0, 320.0);
                egui::ScrollArea::both()
                    .id_salt(outline_id)
                    .min_scrolled_width(0.0)
                    .min_scrolled_height(0.0)
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.set_max_width(820.0);
                        let sheet = d.doc.sheet().clone();
                        let todo_progress = sheet.todo_progress();
                        let numbers = sheet.numbering_labels();
                        let mut rows: Vec<(TopicId, usize)> = Vec::new();
                        fn walk(
                            s: &thinoda_core::Sheet,
                            id: TopicId,
                            depth: usize,
                            out: &mut Vec<(TopicId, usize)>,
                        ) {
                            out.push((id, depth));
                            if s.topic(id).is_some_and(|t| t.collapsed) {
                                return;
                            }
                            for c in s.children(id) {
                                walk(s, *c, depth + 1, out);
                            }
                        }
                        for top in sheet.content_top_level_ids() {
                            walk(&sheet, top, 0, &mut rows);
                        }
                        for (id, depth) in rows {
                            let Some(topic) = sheet.topic(id) else {
                                continue;
                            };
                            ui.horizontal(|ui| {
                                if d.view
                                    .filter
                                    .as_ref()
                                    .is_some_and(|keep| !keep.contains(&id))
                                {
                                    ui.multiply_opacity(0.3);
                                }
                                ui.add_space(depth as f32 * 22.0);
                                let has_kids = !sheet.children(id).is_empty();
                                let chevron = if topic.collapsed {
                                    icons::CHEVRON_RIGHT
                                } else {
                                    icons::CHEVRON_DOWN
                                };
                                if has_kids {
                                    let response =
                                        ui.push_id(("outline_disclosure", id), |ui| {
                                            ui.add(
                                                egui::Button::new(chevron)
                                                    .frame(false)
                                                    .min_size(vec2(16.0, 16.0)),
                                            )
                                        })
                                        .inner
                                        .accessible_name(
                                            format!("{}: {}", t("Collapse / Expand"), topic.title),
                                        );
                                    topic_focus |= focused == Some(response.id);
                                    if response.gained_focus() {
                                        response.scroll_to_me(None);
                                    }
                                    if touch_actions_allowed && response.clicked() {
                                        response.request_focus();
                                        d.view.select_only(id);
                                        action = Some(Action::ToggleCollapse);
                                    }
                                } else {
                                    ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                                }
                                if let Some(number) = numbers.get(&id) {
                                    ui.label(number);
                                }
                                if let Some(metadata) =
                                    sheet.fim.as_ref().and_then(|fim| fim.topic_metadata(id))
                                {
                                    let kind = match metadata.kind {
                                        thinoda_core::fim::FimKind::Field => "F",
                                        thinoda_core::fim::FimKind::Group => "G",
                                        thinoda_core::fim::FimKind::Choice => "G · X",
                                        thinoda_core::fim::FimKind::Rule => "R",
                                        thinoda_core::fim::FimKind::CodeList => "C",
                                        thinoda_core::fim::FimKind::Schema => "S",
                                        _ => "FIM",
                                    };
                                    ui.label(egui::RichText::new(kind).strong()).on_hover_text(
                                        format!(
                                            "{} · {}",
                                            metadata.identity.as_deref().unwrap_or_default(),
                                            metadata.version.as_deref().unwrap_or_default()
                                        ),
                                    );
                                    if let Some(cardinality) = &metadata.cardinality {
                                        ui.label(cardinality).on_hover_text(t("Cardinality"));
                                    }
                                    if metadata.usages > 1 {
                                        ui.label(format!("×{}", metadata.usages))
                                            .on_hover_text(t("Shared definition"));
                                    }
                                }
                                if let Some(todo) = topic.todo {
                                    let progress = todo_progress.get(&id);
                                    let done = progress.map_or(todo, |p| p.complete());
                                    ui.label(if done { "☑" } else { "☐" });
                                    if let Some(p) = progress.filter(|p| p.nested) {
                                        ui.label(format!("{}/{}", p.completed, p.total));
                                    }
                                }
                                for (index, m) in topic.markers.iter().enumerate() {
                                    let mut text = RichText::new(markers::glyph(m));
                                    if let Some(color) = topic.marker_colors.get(&index) {
                                        text = text.color(thinoda_canvas::color32(*color));
                                    }
                                    ui.label(text);
                                }
                                if ui.is_enabled() && d.view.editing_topic() == Some(id) {
                                    let mut text =
                                        d.view.editing_text().unwrap_or_default().to_string();
                                    let te_id = egui::Id::unique(("outliner_edit", id));
                                    d.view.protect_title_ime(ui.ctx(), te_id);
                                    let r = ui
                                        .add(
                                            TextEdit::singleline(&mut text)
                                                .id(te_id)
                                                .lock_focus(true)
                                                .hint_text(t("Topic title"))
                                                .desired_width(edit_width),
                                        )
                                        .accessible_name(t("Topic title"));
                                    if !r.has_focus() && !r.lost_focus() {
                                        r.request_focus();
                                        r.scroll_to_me(None);
                                    } else if r.gained_focus() {
                                        r.scroll_to_me(None);
                                    }
                                    d.view.set_editing_text(&text);
                                    let (enter, tab, esc) = ui.input_mut(|i| {
                                        (
                                            i.consume_key(Modifiers::NONE, Key::Enter),
                                            i.consume_key(Modifiers::NONE, Key::Tab),
                                            i.consume_key(Modifiers::NONE, Key::Escape),
                                        )
                                    });
                                    if enter || tab || esc || r.lost_focus() {
                                        d.view.commit_edit(&mut d.doc);
                                        if esc {
                                            ui.data_mut(|data| {
                                                data.insert_temp(
                                                    outline_id.with("focus_topic"),
                                                    Some(id),
                                                );
                                            });
                                        }
                                        if enter {
                                            action = Some(Action::InsertSibling);
                                        } else if tab {
                                            action = Some(Action::InsertChild);
                                        }
                                    }
                                } else {
                                    let selected = d.view.selection.contains(&id);
                                    let title = if topic.title.is_empty() {
                                        " "
                                    } else {
                                        topic.title.as_str()
                                    };
                                    let text = if depth == 0 {
                                        RichText::new(title).size(18.0).strong()
                                    } else {
                                        RichText::new(title)
                                    };
                                    let r = ui
                                        .push_id(("outline_title", id), |ui| {
                                            ui.selectable_label(selected, text)
                                        })
                                        .inner;
                                    if let Some(record) =
                                        sheet.fim.as_ref().and_then(|fim| fim.record_for_topic(id))
                                    {
                                        r.context_menu(|ui| {
                                            for (label, position) in [
                                                ("Edit FIM node", None),
                                                (
                                                    "Add FIM child",
                                                    Some(crate::fim::FimInsertion::Child),
                                                ),
                                                (
                                                    "Insert FIM node before",
                                                    Some(crate::fim::FimInsertion::Before),
                                                ),
                                                (
                                                    "Insert FIM node after",
                                                    Some(crate::fim::FimInsertion::After),
                                                ),
                                            ] {
                                                if ui.button(t(label)).clicked()
                                                    && touch_actions_allowed
                                                {
                                                    fim_node_request = Some((record, position));
                                                    ui.close();
                                                }
                                            }
                                        });
                                    }
                                    topic_focus |= focused == Some(r.id);
                                    if focus_topic == Some(id) {
                                        r.request_focus();
                                        topic_focus = true;
                                    }
                                    if (primary == Some(id) && reveal_primary) || r.gained_focus() {
                                        r.scroll_to_me(None);
                                    }
                                    if touch_actions_allowed && r.clicked() {
                                        r.request_focus();
                                        d.view.select_only(id);
                                    }
                                    if touch_actions_allowed && r.double_clicked() {
                                        d.view.select_only(id);
                                        d.view.begin_edit(&sheet, id, false);
                                    }
                                }
                                if topic.notes.is_some() {
                                    ui.label(RichText::new(icons::NOTE).weak());
                                }
                                if let Some(texture) = topic
                                    .image
                                    .as_deref()
                                    .filter(|reference| sheet.resources.get(reference).is_ok())
                                    .and_then(|reference| {
                                        thinoda_canvas::resource_preview_texture(
                                            ui.ctx(),
                                            &sheet,
                                            reference,
                                        )
                                    })
                                {
                                    let size = vec2(48.0, 32.0);
                                    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                                    let source = texture.size_vec2();
                                    let scale =
                                        (rect.width() / source.x).min(rect.height() / source.y);
                                    let image_size = source * scale;
                                    let image_rect =
                                        egui::Rect::from_center_size(rect.center(), image_size);
                                    let tint = if d
                                        .view
                                        .filter
                                        .as_ref()
                                        .is_some_and(|keep| !keep.contains(&id))
                                    {
                                        Color32::WHITE.gamma_multiply(0.3)
                                    } else {
                                        Color32::WHITE
                                    };
                                    ui.put(
                                        image_rect,
                                        egui::Image::new((texture.id(), image_size))
                                            .fit_to_exact_size(image_size)
                                            .alt_text(format!(
                                                "{}: {}",
                                                t("Image preview"),
                                                topic.title
                                            ))
                                            .tint(tint),
                                    );
                                }
                            });
                        }
                        if let Some(source) = &sheet.fim {
                            let associated: Vec<_> = source
                                .records()
                                .into_iter()
                                .filter(|record| {
                                    matches!(
                                        record.kind,
                                        thinoda_core::fim::FimKind::Rule
                                            | thinoda_core::fim::FimKind::CodeList
                                    ) && source.topic_for_record(record.record).is_none()
                                })
                                .collect();
                            if !associated.is_empty() {
                                ui.separator();
                                ui.strong(t("FIM data"));
                                for record in associated {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            if record.kind == thinoda_core::fim::FimKind::Rule {
                                                "R"
                                            } else {
                                                "C"
                                            },
                                        );
                                        let response =
                                            ui.add(egui::Button::new(&record.name).wrap());
                                        if response.clicked() {
                                            fim_record = Some(record.record);
                                        }
                                        response.context_menu(|ui| {
                                            if ui.button(t("Edit FIM node")).clicked() {
                                                fim_node_request = Some((record.record, None));
                                                ui.close();
                                            }
                                        });
                                    });
                                }
                            }
                        }
                    });
            });
        // Keyboard in outliner when not editing.
        if ui.is_enabled()
            && !self.search.open
            && d.view.editing_topic().is_none()
            && d.view.editing_board_object().is_none()
            && !ctx.text_edit_focused()
            && !egui::Popup::is_any_open(&ctx)
        {
            let mut navigation = None;
            for (shortcut, a) in &shortcuts {
                if (topic_focus || matches!(a, Action::Undo | Action::Redo))
                    && action.is_none()
                    && ctx.input_mut(|i| i.consume_shortcut(shortcut))
                {
                    match a {
                        Action::Navigate(
                            direction @ (thinoda_core::navigation::Direction::Up
                            | thinoda_core::navigation::Direction::Down),
                        ) => navigation = Some(*direction),
                        _ => action = Some(a.clone()),
                    }
                    break;
                }
            }
            if topic_focus
                && action.is_none()
                && let Some(direction) = navigation
                && let Some(cur) = d.view.primary()
            {
                let sheet = d.doc.sheet();
                let order = sheet.visible_topic_ids();
                if let Some(i) = order.iter().position(|x| *x == cur) {
                    let j = if direction == thinoda_core::navigation::Direction::Up {
                        i.saturating_sub(1)
                    } else {
                        (i + 1).min(order.len() - 1)
                    };
                    d.view.select_only(order[j]);
                    ui.data_mut(|data| {
                        data.insert_temp(outline_id.with("focus_topic"), Some(order[j]));
                    });
                }
            }
            if topic_focus && action.is_none() {
                action = ctx.input(|input| {
                    input.events.iter().find_map(|event| match event {
                        egui::Event::Copy => Some(Action::Copy),
                        egui::Event::Cut => Some(Action::Cut),
                        egui::Event::Paste(text) => Some(Action::Paste(Some(text.clone()))),
                        _ => None,
                    })
                });
            }
        }
        if let Some(a) = action {
            self.canvas_action(&ctx, a);
            if topic_focus
                && let Some(doc) = self.doc()
                && doc.view.editing_topic().is_none()
            {
                ui.data_mut(|data| {
                    data.insert_temp(outline_id.with("focus_topic"), doc.view.primary());
                });
            }
            if let Some(text) = self.doc_mut().and_then(|doc| doc.view.clipboard_out.take()) {
                ctx.copy_text(text);
            }
        }
        if let Some(record) = fim_record {
            self.select_fim_record(&ctx, record);
        }
        if let Some((record, position)) = fim_node_request {
            self.begin_fim_node(&ctx, Some(record), position);
        }
    }

    // ----- bottom bar ---------------------------------------------------------------------------

    fn bottom_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let touch_allowed = widgets::touch_activation_allowed(&ctx);
        let mut cmd = None;
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        ui.horizontal_centered(|ui| {
            let add_width = ui.spacing().interact_size.x;
            let height = ui.spacing().interact_size.y;
            let show_badge = ui.available_width() >= crate::brand::GITHUB_BADGE_MIN_BAR_WIDTH;
            let badge_width = if show_badge { height } else { 0.0 };
            let gaps =
                1.0 + if self.settings.show_status_bar {
                    1.0
                } else {
                    0.0
                } + if show_badge { 1.0 } else { 0.0 };
            let width = (ui.available_width()
                - add_width
                - badge_width
                - gaps * ui.spacing().item_spacing.x)
                .max(0.0);
            let tabs_width = if self.settings.show_status_bar {
                // Keep a whole truncated tab visible between the arrows and status controls.
                let min_tabs_width = 2.0 * (height + ui.spacing().item_spacing.x)
                    + ui.spacing().interact_size.x
                    + 2.0 * Tokens::SPACE_XS
                    + ui.spacing().item_spacing.x;
                (width * 0.4).max(min_tabs_width.min(width))
            } else {
                width
            };
            sheet_scroll_row(ui, tabs_width, |ui, names_width| {
                // Sheet tabs.
                let n = d.doc.workbook().sheets.len();
                let active_index = d.doc.active_index();
                let active = egui::Id::unique((
                    &d.key,
                    active_index,
                    names_width.to_bits(),
                    d.doc
                        .workbook()
                        .sheets
                        .iter()
                        .map(|sheet| &sheet.title)
                        .collect::<Vec<_>>(),
                ));
                let active_key = ui.scope_id().with("active_sheet");
                let changed = ui.data_mut(|data| {
                    let previous = data.get_temp::<egui::Id>(active_key);
                    data.insert_temp(active_key, active);
                    previous != Some(active)
                });
                let max_width =
                    (names_width - 2.0 * Tokens::SPACE_XS - ui.spacing().item_spacing.x)
                        .clamp(0.0, 180.0);
                for i in 0..n {
                    let title = d.doc.workbook().sheets[i].title.clone();
                    let text = egui::WidgetText::from(title.clone()).into_galley(
                        ui,
                        Some(egui::TextWrapMode::Extend),
                        f32::INFINITY,
                        egui::TextStyle::Button,
                    );
                    let tab_width =
                        (text.size().x + 2.0 * ui.spacing().button_padding.x).min(max_width);
                    let r = ui
                        .add_sized(
                            vec2(tab_width, height),
                            egui::Button::selectable(i == d.doc.active_index(), &title).truncate(),
                        )
                        .on_hover_text(&title);
                    if r.gained_focus() || (changed && i == active_index) {
                        r.scroll_to_me(None);
                    }
                    if touch_allowed && (r.clicked() || r.double_clicked()) {
                        match d.switch_sheet(i) {
                            Ok(_) if r.double_clicked() => cmd = Some(Cmd::RenameSheet),
                            Ok(_) => {}
                            Err(error) => {
                                self.toasts
                                    .error(format!("{}: {error}", t("Not possible here")));
                            }
                        }
                    }
                    let mut popup = egui::Popup::context_menu(&r);
                    if !touch_allowed {
                        popup = popup.open_memory(None);
                    }
                    popup.show(|ui| {
                        if touch_allowed && let Err(error) = d.switch_sheet(i) {
                            self.toasts
                                .error(format!("{}: {error}", t("Not possible here")));
                            ui.close();
                            return;
                        }
                        scroll_menu(ui, |ui| {
                            // Create, edit, clipboard, order, export, then the destructive row.
                            for group in [
                                &[("New Sheet", Cmd::NewSheet)][..],
                                &[
                                    ("Rename Sheet", Cmd::RenameSheet),
                                    ("Duplicate Sheet", Cmd::DuplicateSheet),
                                ],
                                &[
                                    ("Copy Sheet", Cmd::CopySheet),
                                    ("Paste Sheet", Cmd::PasteSheet),
                                ],
                            ] {
                                for (label, command) in group {
                                    if ui.button(t(label)).clicked() && touch_allowed {
                                        cmd = Some(command.clone());
                                    }
                                }
                                ui.separator();
                            }
                            if ui
                                .add_enabled(i > 0, egui::Button::new(t("Move Left")))
                                .clicked()
                                && touch_allowed
                            {
                                match d
                                    .commit_canvas_drafts()
                                    .and_then(|()| d.flush_inspector_edit(ui.ctx()))
                                    .and_then(|()| {
                                        d.doc
                                            .move_sheet(i, i - 1)
                                            .map_err(|error| error.to_string())
                                    }) {
                                    Ok(()) => {
                                        d.view.reset();
                                        ui.close();
                                    }
                                    Err(error) => {
                                        self.toasts
                                            .error(format!("{}: {error}", t("Not possible here")));
                                    }
                                }
                            }
                            if ui
                                .add_enabled(i + 1 < n, egui::Button::new(t("Move Right")))
                                .clicked()
                                && touch_allowed
                            {
                                match d
                                    .commit_canvas_drafts()
                                    .and_then(|()| d.flush_inspector_edit(ui.ctx()))
                                    .and_then(|()| {
                                        d.doc
                                            .move_sheet(i, i + 1)
                                            .map_err(|error| error.to_string())
                                    }) {
                                    Ok(()) => {
                                        d.view.reset();
                                        ui.close();
                                    }
                                    Err(error) => {
                                        self.toasts
                                            .error(format!("{}: {error}", t("Not possible here")));
                                    }
                                }
                            }
                            ui.separator();
                            if ui.button(t("Save Sheet As…")).clicked() && touch_allowed {
                                cmd = Some(Cmd::SaveSheetAs);
                            }
                            ui.separator();
                            if ui
                                .add_enabled(n > 1, egui::Button::new(t("Delete Sheet")))
                                .clicked()
                                && touch_allowed
                            {
                                cmd = Some(Cmd::DeleteSheet);
                            }
                        });
                    });
                }
            });
            ui.allocate_ui_with_layout(
                vec2(add_width, height),
                Layout::left_to_right(Align::Center),
                |ui| {
                    if ui
                        .small_button(icons::ADD)
                        .on_hover_text(t("New Sheet"))
                        .accessible_name(t("New Sheet"))
                        .clicked()
                        && touch_allowed
                    {
                        cmd = Some(Cmd::NewSheet);
                    }
                },
            );
            if self.settings.show_status_bar {
                scroll_row(ui, "bottom_controls", width - tabs_width, |ui| {
                    let (icon, current, next) = match self.mode {
                        CenterMode::Map => (icons::MAP, "Map", "Outliner"),
                        CenterMode::Outliner => (icons::OUTLINER, "Outliner", "Map"),
                    };
                    let label = format!("{icon} {}", t(current));
                    let text = egui::WidgetText::from(label.clone()).into_galley(
                        ui,
                        Some(egui::TextWrapMode::Extend),
                        f32::INFINITY,
                        egui::TextStyle::Button,
                    );
                    let view_width = (text.size().x + 2.0 * ui.spacing().button_padding.x)
                        .min(width - tabs_width);
                    let view = ui
                        .add_sized(
                            vec2(view_width, height),
                            egui::Button::new(label).truncate(),
                        )
                        .accessible_name(format!("{}: {}", t("View"), t(current)))
                        .on_hover_text(format!(
                            "{} ({})",
                            t(next),
                            ctx.format_shortcut(&egui::KeyboardShortcut::new(
                                Modifiers::COMMAND.plus(Modifiers::SHIFT),
                                Key::O
                            ),)
                        ));
                    if view.gained_focus() {
                        view.scroll_to_me(None);
                    }
                    if view.clicked() && touch_allowed {
                        cmd = Some(Cmd::ToggleOutliner);
                    }
                    widgets::vdivider(ui);
                    let minus = ui
                        .small_button(icons::ZOOM_OUT)
                        .on_hover_text(t("Zoom Out"))
                        .accessible_name(t("Zoom Out"));
                    if minus.gained_focus() {
                        minus.scroll_to_me(None);
                    }
                    if minus.clicked() && touch_allowed {
                        cmd = Some(Cmd::Canvas(Action::ZoomOut));
                    }
                    let current = d.view.zoom_percent();
                    let zoom = status_menu_button(ui, format!("{current}%"), |ui| {
                        let mut presets = vec![500, 400, 300, 200, 150, 120, 100, 80, 50, 20, 10];
                        if !presets.contains(&current) {
                            presets.push(current);
                            presets.sort_unstable_by(|a, b| b.cmp(a));
                        }
                        for percent in presets {
                            if ui
                                .selectable_label(current == percent, format!("{percent}%"))
                                .clicked()
                                && touch_allowed
                            {
                                d.view.set_zoom(percent as f32 / 100.0);
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button(t("Fit Map")).clicked() && touch_allowed {
                            cmd = Some(Cmd::Canvas(Action::Fit));
                            ui.close();
                        }
                    })
                    .response
                    .accessible_name(t("Zoom"));
                    if zoom.gained_focus() {
                        zoom.scroll_to_me(None);
                    }
                    let plus = ui
                        .small_button(icons::ZOOM_IN)
                        .on_hover_text(t("Zoom In"))
                        .accessible_name(t("Zoom In"));
                    if plus.gained_focus() {
                        plus.scroll_to_me(None);
                    }
                    if plus.clicked() && touch_allowed {
                        cmd = Some(Cmd::Canvas(Action::ZoomIn));
                    }
                    let fit = ui
                        .small_button(icons::FIT)
                        .on_hover_text(t("Fit Map"))
                        .accessible_name(t("Fit Map"));
                    if fit.gained_focus() {
                        fit.scroll_to_me(None);
                    }
                    if fit.clicked() && touch_allowed {
                        cmd = Some(Cmd::Canvas(Action::Fit));
                    }
                    widgets::vdivider(ui);
                    let sheet = d.doc.sheet();
                    widgets::caption(
                        ui,
                        format!(
                            "{}: {}",
                            t("Words"),
                            thinoda_core::search::word_count(sheet)
                        ),
                    );
                    let statistics = status_menu_button(
                    ui,
                    format!("{}: {}", t("Topics"), sheet.topic_count()),
                    |ui| {
                        ui.strong(t("Current sheet"));
                        for (label, value) in [
                            ("Topics", sheet.topic_count()),
                            ("Words", thinoda_core::search::word_count(sheet)),
                            (
                                "Characters",
                                sheet
                                    .content_topics()
                                    .map(|topic| topic.title.chars().count())
                                    .sum(),
                            ),
                        ] {
                            ui.label(format!("{}: {value}", t(label)));
                        }
                        ui.separator();
                        widgets::caption(
                            ui,
                            t("Topic titles only, including collapsed and floating topics."),
                        );
                        widgets::caption(
                            ui,
                            t("Words split at whitespace; characters count Unicode scalar values."),
                        );
                    },
                )
                .response
                .accessible_name(t("Statistics"));
                    if statistics.gained_focus() {
                        statistics.scroll_to_me(None);
                    }
                    widgets::vdivider(ui);
                    if let Some(c) = d.path.as_ref().and_then(|p| {
                        thinoda_platform::cloud::provider_for_path(&self.services.cloud, p)
                    }) {
                        widgets::caption(ui, format!("{} {}", icons::CLOUD, c.name));
                        #[cfg(not(target_arch = "wasm32"))]
                        if self.settings.cloud.show_cloud_badges
                            && let Some(path) = &d.path
                        {
                            let state = match thinoda_platform::fs::online_only(path) {
                                Ok(true) => t("Online-only"),
                                Ok(false) => t("Local copy; sync status unavailable"),
                                Err(_) => t("Sync status unavailable"),
                            };
                            widgets::caption(ui, state);
                        }
                    }
                    widgets::caption(
                        ui,
                        if d.is_dirty() {
                            t("Unsaved changes")
                        } else {
                            t("Saved")
                        },
                    );
                });
            }
            if show_badge {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    crate::brand::github_badge(ui, badge_width);
                });
            }
        });
        if let Some(c) = cmd {
            self.exec(&ctx, c);
        }
    }

    // ----- format panel -------------------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut Ui) {
        match self.sidebar_family {
            SidebarFamily::Format => self.format_panel(ui),
            SidebarFamily::Symbols => {
                ui.heading(t("Symbols"));
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .id_salt("symbols_scroll")
                    .show(ui, |ui| {
                        self.symbols_panel(ui);
                        reveal_changed_focus(ui);
                    });
            }
        }
    }

    fn symbols_panel(&mut self, ui: &mut Ui) {
        let selection = self
            .doc()
            .map(|d| d.view.selection.clone())
            .unwrap_or_default();
        if selection.is_empty() {
            widgets::caption(ui, t("Select a topic to add symbols."));
        } else {
            widgets::caption(ui, t("Click a symbol to add or remove it. One per group."));
        }
        let mut command = None;
        self.symbol_insertion_color(ui);
        for group in markers::GROUPS {
            widgets::section(ui, t(group.name), true, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for marker in group.markers {
                        let selected = !selection.is_empty()
                            && self.doc().is_some_and(|d| {
                                selection.iter().all(|id| {
                                    d.doc.sheet().topic(*id).is_some_and(|topic| {
                                        topic.markers.iter().any(|m| m == marker.id)
                                    })
                                })
                            });
                        if self.symbol_button(ui, marker, selected) {
                            command = Some(Cmd::Canvas(Action::ToggleMarkerColor(
                                marker.id.into(),
                                self.symbol_color,
                            )));
                        }
                    }
                });
            });
        }
        self.placed_symbols(ui, &mut command);
        if let Some(command) = command {
            let ctx = ui.ctx().clone();
            self.exec(&ctx, command);
        }
    }

    fn format_panel(&mut self, ui: &mut Ui) {
        let mut tab = self.format_tab;
        widgets::segmented(
            ui,
            &mut tab,
            &[
                (FormatTab::Style, t("Style")),
                (FormatTab::Data, t("Data")),
                (FormatTab::Map, t("Map")),
                (FormatTab::Fim, "FIM"),
            ],
        );
        if self.format_tab != tab {
            let result = self
                .doc_mut()
                .map_or(Ok(()), |doc| doc.flush_inspector_edit(ui.ctx()));
            match result {
                Ok(()) => {
                    self.format_tab = tab;
                    self.save_settings();
                }
                Err(error) => {
                    self.toasts.error(error);
                    tab = self.format_tab;
                }
            }
        }
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .auto_shrink([tab != FormatTab::Fim, true])
            .id_salt(match tab {
                FormatTab::Style => "style_scroll",
                FormatTab::Data => "data_scroll",
                FormatTab::Map => "map_scroll",
                FormatTab::Fim => "fim_scroll",
            })
            .show(ui, |ui| {
                ui.push_id(
                    match tab {
                        FormatTab::Style => "style_content",
                        FormatTab::Data => "data_content",
                        FormatTab::Map => "map_content",
                        FormatTab::Fim => "fim_content",
                    },
                    |ui| match tab {
                        FormatTab::Style => self.style_tab(ui),
                        FormatTab::Data => self.data_tab(ui),
                        FormatTab::Map => self.map_tab(ui),
                        FormatTab::Fim => self.fim_tab(ui),
                    },
                );
                reveal_changed_focus(ui);
            });
    }

    pub(crate) fn image_properties(&mut self, ui: &mut Ui) {
        let Some(doc) = self.doc() else { return };
        let Some(id) = doc.view.primary() else { return };
        let Some(topic) = doc.doc.sheet().topic(id).cloned() else {
            return;
        };
        let enabled = self.command_available(&Cmd::LocalImage) && !self.palette.open;
        let owner = enabled.then(|| {
            egui::Id::unique((
                "image-size",
                &doc.key,
                doc.doc.active_index(),
                id,
                doc.doc.state_id(),
            ))
        });
        let has_image = topic.image.is_some();
        let mut pick = false;
        let mut remove = false;
        let mut reset = false;
        let mut width = None;
        let mut below = topic.image_below;
        widgets::section(ui, t("Local Image"), true, |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                pick = ui
                    .button(if has_image {
                        t("Replace image")
                    } else {
                        t("Insert image")
                    })
                    .clicked();
                if has_image {
                    if let Some(resource) = topic
                        .image
                        .as_deref()
                        .and_then(|r| self.doc()?.doc.sheet().resources.get(r).ok())
                    {
                        ui.label(&resource.name);
                    }
                    let mut value = topic
                        .image_size
                        .unwrap_or(thinoda_core::layout::IMAGE_SIZE)
                        .x;
                    widgets::property_field(ui, t("Image width"), |ui| {
                        let (response, commit) = touch_numeric_value(
                            ui,
                            &mut value,
                            "image-width",
                            owner,
                            |ui, value| {
                                ui.add(
                                    egui::DragValue::new(value)
                                        .range(0.01..=1024.0)
                                        .fixed_decimals(2)
                                        .speed(1.0),
                                )
                            },
                        );
                        if commit
                            && value
                                != topic
                                    .image_size
                                    .unwrap_or(thinoda_core::layout::IMAGE_SIZE)
                                    .x
                        {
                            width = Some(value);
                        }
                        response
                    });
                    ui.checkbox(&mut below, t("Image below topic"));
                    reset = ui.button(t("Reset Size")).clicked();
                    remove = ui.button(t("Remove image")).clicked();
                }
                widgets::caption(ui, t("PNG/JPEG only. Resizing keeps the aspect ratio."));
            });
        });
        let activate = widgets::touch_activation_allowed(ui.ctx()) && enabled;
        if activate && pick {
            self.begin_image(ui.ctx());
        }
        if activate && (remove || reset || width.is_some() || below != topic.image_below) {
            let result = (|| -> Result<(), String> {
                let doc = self.doc_mut().ok_or("image document is closed")?;
                doc.flush_inspector_edit(ui.ctx())?;
                let size = if reset || width.is_some() {
                    let resource = doc
                        .doc
                        .sheet()
                        .resources
                        .get(topic.image.as_deref().ok_or("image is missing")?)?;
                    let decoded = thinoda_core::images::decode(resource.bytes())?;
                    let mut size =
                        thinoda_core::images::default_size(decoded.width, decoded.height);
                    if let Some(width) = width {
                        let ratio = decoded.height as f32 / decoded.width as f32;
                        let width = width.min(1024.0 / ratio).clamp(f32::MIN_POSITIVE, 1024.0);
                        size = thinoda_core::Vec2::new(width, width * ratio);
                    }
                    Some(size)
                } else {
                    topic.image_size
                };
                doc.doc
                    .edit_sheet("Local Image", |sheet| {
                        if remove {
                            thinoda_core::images::set_image(sheet, id, None)
                                .map_err(thinoda_core::edit::EditError::Invalid)?;
                        } else if let Some(t) = sheet.topics.get_mut(&id) {
                            t.image_size = size;
                            t.image_below = below;
                        }
                        Ok(())
                    })
                    .map_err(|e| e.to_string())?;
                Ok(())
            })();
            if let Err(error) = result {
                self.toasts.error(error);
            }
        }
    }

    fn board_style_controls(&mut self, ui: &mut Ui) {
        let Some(doc) = self.doc() else { return };
        let sheet = doc.doc.sheet();
        let Some(targets) = StyleTargets::from_view(sheet, &doc.view) else {
            return;
        };
        if !targets.styleable_board && targets.topics.is_empty() {
            return;
        }
        let fill = targets.common_color(sheet, doc.view.dark, StyleColor::Fill);
        let border = targets.common_color(sheet, doc.view.dark, StyleColor::Border);
        let text = targets.common_color(sheet, doc.view.dark, StyleColor::Text);
        let fill_enabled = targets.supports_color(sheet, StyleColor::Fill);
        let border_enabled = targets.supports_color(sheet, StyleColor::Border);
        let text_enabled = targets.supports_color(sheet, StyleColor::Text);
        let single = targets.topics.len() + targets.shapes.len() == 1;
        if targets.locked {
            widgets::caption(ui, t("Locked"));
        }
        let mut change = None;
        widgets::section(ui, t("Shape"), true, |ui| {
            ui.add_enabled_ui(fill_enabled, |ui| {
                if let Some(color) = style_color_row(ui, "Fill", fill, single && fill.is_none()) {
                    change = Some((StyleColor::Fill, color));
                }
            });
            ui.add_enabled_ui(border_enabled, |ui| {
                if let Some(color) =
                    style_color_row(ui, "Border", border, single && border.is_none())
                {
                    change = Some((StyleColor::Border, color));
                }
            });
        });
        widgets::section(ui, t("Text"), true, |ui| {
            ui.add_enabled_ui(text_enabled, |ui| {
                if let Some(color) = style_color_row(ui, "Text Color", text, false) {
                    change = Some((StyleColor::Text, color));
                }
            });
        });
        if let Some((field, color)) = change {
            self.apply_style_color(field, color);
        }
    }

    fn style_tab(&mut self, ui: &mut Ui) {
        if self
            .doc()
            .is_some_and(|doc| !doc.view.board_selection.is_empty())
        {
            self.quick_styles(ui);
            self.board_style_controls(ui);
            self.board_object_properties(ui);
            return;
        }
        if let Some(thinoda_canvas::ObjectSelection::Zone(id)) =
            self.doc().and_then(|doc| doc.view.selected_object)
        {
            self.zone_properties(ui, id);
            return;
        }
        let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
        let numeric_enabled = !self.any_modal_open()
            && !self.dialogs.settings
            && !self.dialogs.about
            && !self.palette.open;
        if let Some(thinoda_canvas::ObjectSelection::Relationship(id)) =
            self.doc().and_then(|d| d.view.selected_object)
        {
            self.relationship_style(ui, id, touch_allowed);
            return;
        }
        self.board_card_properties(ui);
        self.quick_styles(ui);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        let targets = StyleTargets::from_view(d.doc.sheet(), &d.view);
        let sel = targets
            .as_ref()
            .filter(|targets| targets.board)
            .map_or_else(
                || d.view.selection.clone(),
                |targets| targets.topics.clone(),
            );
        if targets.as_ref().is_some_and(|targets| targets.locked) {
            widgets::caption(ui, t("Locked"));
        }
        let Some(&primary) = sel.last() else {
            if !targets.as_ref().is_some_and(|targets| targets.locked) {
                widgets::caption(ui, t("Select a topic to edit its style."));
            }
            return;
        };
        let sheet = d.doc.sheet();
        let Some(topic) = sheet.topic(primary).cloned() else {
            return;
        };
        let resolved = thinoda_core::theme::resolve_style(sheet, primary, d.view.dark);
        let numeric_owner = numeric_enabled.then(|| {
            egui::Id::unique("style_numeric_owner").with((
                &d.key,
                d.doc.active_index(),
                sheet.root,
                &sel,
                d.doc.state_id(),
            ))
        });
        let can_set_branch_side = sel.iter().all(|id| sheet.parent(*id) == Some(sheet.root))
            && matches!(
                sheet.structure,
                Structure::MapClockwise | Structure::MapAnticlockwise | Structure::MapUnbalanced
            );
        let can_number = sel.iter().all(|id| sheet.parent(*id).is_some());
        let numbering = sheet.sibling_numbering(primary);
        let mut numbering_change: Option<Option<Numbering>> = None;
        let mut change: Option<StyleChange> = None;
        let mut color_change = None;
        let mut reset_style = false;
        let has_cards = sel
            .iter()
            .any(|id| sheet.topic(*id).is_some_and(|topic| topic.card.is_some()));
        let fill = targets
            .as_ref()
            .filter(|targets| targets.board)
            .map_or(resolved.fill, |targets| {
                targets.common_color(sheet, d.view.dark, StyleColor::Fill)
            });
        let text_color = targets
            .as_ref()
            .filter(|targets| targets.board)
            .map_or(Some(resolved.text_color), |targets| {
                targets.common_color(sheet, d.view.dark, StyleColor::Text)
            });
        let border_color = targets
            .as_ref()
            .filter(|targets| targets.board)
            .map_or(resolved.border_color, |targets| {
                targets.common_color(sheet, d.view.dark, StyleColor::Border)
            });
        let border_enabled = targets
            .as_ref()
            .filter(|targets| targets.board)
            .is_none_or(|targets| targets.supports_color(sheet, StyleColor::Border));
        ui.label(RichText::new(&topic.title).strong());
        if sel.len() > 1 {
            widgets::caption(
                ui,
                t("Showing the primary topic; edits apply to all selected topics."),
            );
        }

        widgets::section(ui, t("Structure"), true, |ui| {
            if can_set_branch_side {
                let mut side = topic.branch_side;
                widgets::property_field(ui, t("Branch side"), |ui| {
                    egui::ComboBox::from_id_salt(("branch_side", primary))
                        .selected_text(t(match side {
                            None => "Auto",
                            Some(thinoda_core::BranchSide::Left) => "Left",
                            Some(thinoda_core::BranchSide::Right) => "Right",
                        }))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (None, "Auto"),
                                (Some(thinoda_core::BranchSide::Left), "Left"),
                                (Some(thinoda_core::BranchSide::Right), "Right"),
                            ] {
                                ui.selectable_value(&mut side, value, t(label));
                            }
                        })
                        .response
                });
                if touch_allowed && side != topic.branch_side {
                    let result = d.doc.edit_sheet("Branch side", |s| {
                        for id in &sel {
                            s.set_branch_side(*id, side)?;
                        }
                        Ok(())
                    });
                    if let Err(error) = result {
                        self.toasts
                            .warning(format!("{}: {error}", t("Not possible here")));
                    }
                }
            }
            let cur = topic.structure;
            let mut chosen = None;
            widgets::property_field(ui, t("Structure"), |ui| {
                egui::ComboBox::from_id_salt("topic_structure")
                    .selected_text(cur.map_or(t("Auto"), |s| t(structure_label(s))))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(cur.is_none(), t("Auto")).clicked() {
                            chosen = Some(None);
                        }
                        for s in Structure::ALL {
                            if ui
                                .selectable_label(cur == Some(s), t(structure_label(s)))
                                .clicked()
                            {
                                chosen = Some(Some(s));
                            }
                        }
                    })
                    .response
            });
            if touch_allowed && let Some(s) = chosen {
                let _ = d
                    .doc
                    .edit_sheet("Structure", |sh| sh.set_structure(Some(primary), s));
            }
        });

        widgets::section(ui, t("Shape"), true, |ui| {
            let mut shape = resolved.shape;
            ui.add_enabled_ui(!has_cards, |ui| {
                widgets::property_field(ui, t("Shape"), |ui| {
                    egui::ComboBox::from_id_salt("topic_shape")
                        .selected_text(t(shape.label()))
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for s in TopicShape::ALL {
                                ui.selectable_value(&mut shape, s, t(s.label()));
                            }
                        })
                        .response
                });
            });
            if touch_allowed && shape != resolved.shape {
                change = Some(("Shape", Box::new(move |st| st.shape = Some(shape))));
            }
            if let Some(color) =
                style_color_row(ui, "Fill", fill, !has_cards && resolved.fill.is_none())
            {
                color_change = Some((StyleColor::Fill, color));
            }
            ui.add_enabled_ui(border_enabled, |ui| {
                if let Some(color) =
                    style_color_row(ui, "Border", border_color, border_color.is_none())
                {
                    color_change = Some((StyleColor::Border, color));
                }
            });
            ui.add_enabled_ui(!has_cards, |ui| {
                let mut bw = resolved.border_width;
                widgets::property_field(ui, t("Width"), |ui| {
                    let (r, commit) = touch_numeric_value(
                        ui,
                        &mut bw,
                        "border-width",
                        numeric_owner,
                        |ui, value| ui.add(egui::Slider::new(value, 0.0..=8.0).step_by(0.5)),
                    );
                    if commit && (r.changed() || bw != resolved.border_width) {
                        change = Some(("Border", Box::new(move |st| st.border_width = Some(bw))));
                    }
                    r
                });
            });
        });

        widgets::section(ui, t("Text"), true, |ui| {
            let mut family = topic.style.font_family.clone();
            widgets::property_field(ui, t("Font Family"), |ui| {
                font_selector(ui, &mut self.fonts, &mut family, t("Auto"))
            });
            if touch_allowed && family != topic.style.font_family {
                change = Some((
                    "Font Family",
                    Box::new(move |st| st.font_family = family.clone()),
                ));
            }
            let mut size = resolved.font_size;
            widgets::property_field(ui, t("Font Size"), |ui| {
                let (r, commit) =
                    touch_numeric_value(ui, &mut size, "font-size", numeric_owner, |ui, value| {
                        ui.add(egui::DragValue::new(value).range(6.0..=96.0).speed(0.5))
                    });
                if commit && (r.changed() || size != resolved.font_size) {
                    change = Some(("Font Size", Box::new(move |st| st.font_size = Some(size))));
                }
                r
            });
            ui.horizontal(|ui| {
                let toggles: [(&str, &str, bool, StyleToggle); 4] = [
                    (icons::BOLD, "Bold", resolved.bold, |s, v| s.bold = Some(v)),
                    (icons::ITALIC, "Italic", resolved.italic, |s, v| {
                        s.italic = Some(v)
                    }),
                    (icons::UNDERLINE, "Underline", resolved.underline, |s, v| {
                        s.underline = Some(v)
                    }),
                    (
                        icons::STRIKE,
                        "Strikethrough",
                        resolved.strikethrough,
                        |s, v| s.strikethrough = Some(v),
                    ),
                ];
                for (icon, label, on, set) in toggles {
                    if widgets::icon_button(ui, icon, t(label), on).clicked() && touch_allowed {
                        change = Some(("Text Style", Box::new(move |st| set(st, !on))));
                    }
                }
                widgets::vdivider(ui);
                for (a, icon, label) in [
                    (TextAlign::Left, icons::TEXT_ALIGN_LEFT, "Align Left"),
                    (TextAlign::Center, icons::TEXT_ALIGN_CENTER, "Align Center"),
                    (TextAlign::Right, icons::TEXT_ALIGN_RIGHT, "Align Right"),
                ] {
                    if widgets::icon_button(ui, icon, t(label), resolved.align == a).clicked()
                        && touch_allowed
                    {
                        change = Some(("Align", Box::new(move |st| st.align = Some(a))));
                    }
                }
            });
            if let Some(color) = style_color_row(ui, "Text Color", text_color, false) {
                color_change = Some((StyleColor::Text, color));
            }
        });

        widgets::section(ui, t("Numbering"), true, |ui| {
            if !can_number {
                widgets::caption(
                    ui,
                    t("Select a subtopic to number it together with its siblings."),
                );
                return;
            }
            let mut next = numbering;
            widgets::property_field(ui, t("Numbering"), |ui| {
                egui::ComboBox::from_id_salt(("topic_numbering", primary))
                    .selected_text(numbering.map_or(t("None"), |n| n.format.sample()))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut next, None, t("None"));
                        for format in NumberFormat::ALL {
                            let value = Some(Numbering {
                                format,
                                ..numbering.unwrap_or_default()
                            });
                            ui.selectable_value(&mut next, value, format.sample());
                        }
                    })
                    .response
            });
            if let Some(current) = numbering {
                let mut tiered = current.tiered;
                if ui.checkbox(&mut tiered, t("Tiered numbers")).changed() {
                    next = Some(Numbering { tiered, ..current });
                }
                let mut start = current.start as f32;
                widgets::property_field(ui, t("Start at"), |ui| {
                    let (r, commit) = touch_numeric_value(
                        ui,
                        &mut start,
                        "numbering-start",
                        numeric_owner,
                        |ui, value| {
                            ui.add(
                                egui::DragValue::new(value)
                                    .range(1.0..=thinoda_core::numbering::MAX_START as f32)
                                    .speed(0.2)
                                    .fixed_decimals(0),
                            )
                        },
                    );
                    let start = start.round() as u32;
                    if commit && start != current.start {
                        next = Some(Numbering { start, ..current });
                    }
                    r
                });
            }
            widgets::caption(
                ui,
                t("Applies to the selected topic and its siblings; numbers update as topics move."),
            );
            if touch_allowed && next != numbering {
                numbering_change = Some(next);
            }
        });

        widgets::section(ui, t("Branch"), true, |ui| {
            let mut ls = resolved.line_shape;
            widgets::property_field(ui, t("Line"), |ui| {
                egui::ComboBox::from_id_salt("line_shape")
                    .selected_text(t(ls.label()))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for s in LineShape::ALL {
                            ui.selectable_value(&mut ls, s, t(s.label()));
                        }
                    })
                    .response
            });
            if touch_allowed && ls != resolved.line_shape {
                change = Some(("Line", Box::new(move |st| st.line_shape = Some(ls))));
            }
            widgets::property_row(ui, t("Line Color"), |ui| {
                if let Some(c) =
                    widgets::color_swatches(ui, &SWATCHES, Some(from_color(resolved.line_color)))
                {
                    change = Some((
                        "Line Color",
                        Box::new(move |st| st.line_color = Some(to_color(c))),
                    ));
                }
            });
            let mut lw = resolved.line_width;
            widgets::property_field(ui, t("Line Width"), |ui| {
                let (r, commit) =
                    touch_numeric_value(ui, &mut lw, "line-width", numeric_owner, |ui, value| {
                        ui.add(egui::Slider::new(value, 0.5..=8.0).step_by(0.5))
                    });
                if commit && (r.changed() || lw != resolved.line_width) {
                    change = Some(("Line Width", Box::new(move |st| st.line_width = Some(lw))));
                }
                r
            });
        });

        if (!topic.style.is_default() || topic.card.is_some_and(|card| card.color.is_some()))
            && ui.button(t("Reset Style")).clicked()
            && touch_allowed
        {
            reset_style = true;
        }

        let mut marker_action = None;
        if !topic.markers.is_empty() {
            ui.label(t("Markers"));
            ui.horizontal_wrapped(|ui| {
                for (index, m) in topic.markers.iter().enumerate() {
                    if ui.button(format!("{} ✖", markers::glyph(m))).clicked() && touch_allowed {
                        marker_action = Some(Cmd::Canvas(Action::RemoveMarker(primary, index)));
                    }
                }
            });
        }
        if let Some((label, f)) = change {
            let _ = d
                .doc
                .edit_sheet(label, |s| s.update_style(&sel, |st| f(st)));
        }
        if let Some(numbering) = numbering_change {
            let result = d.doc.edit_sheet("Numbering", |s| {
                for id in &sel {
                    s.set_sibling_numbering(*id, numbering)?;
                }
                Ok(())
            });
            if let Err(error) = result {
                self.toasts
                    .warning(format!("{}: {error}", t("Not possible here")));
            }
        }
        if let Some((field, color)) = color_change {
            self.apply_style_color(field, color);
        }
        if reset_style {
            self.reset_selection_style();
        }
        if let Some(command) = marker_action {
            self.exec(ui.ctx(), command);
        }
        self.todo_properties(ui);
    }

    fn data_tab(&mut self, ui: &mut Ui) {
        let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
        // Object styling stays in Style; Data only edits the selected topic.
        if self.doc().is_some_and(|d| d.view.selected_object.is_some()) {
            widgets::caption(ui, t("Select a topic to edit its data."));
            return;
        }
        self.image_properties(ui);
        if self.doc().is_some_and(|doc| {
            doc.view
                .primary()
                .and_then(|id| doc.doc.sheet().topic(id))
                .is_some_and(|topic| topic.attachment.is_some())
        }) {
            data_divider(ui);
        }
        self.attachment_properties(ui);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        let sel = d.view.selection.clone();
        let Some(&primary) = sel.last() else {
            widgets::caption(ui, t("Select a topic to edit its data."));
            return;
        };
        let Some(topic) = d.doc.sheet().topic(primary).cloned() else {
            return;
        };
        let focus = self.focus_field.take();
        let mut edit_labels = false;
        data_section(ui, t("Hyperlink"), |ui, heading| {
            let current = topic.hyperlink.clone().unwrap_or_default();
            let salt = (
                "hyperlink",
                &d.key,
                d.doc.active_index(),
                d.doc.sheet().root,
                primary,
            );
            restore_inspector_buffer(d, ui, salt, primary, &crate::doc::InspectorField::Hyperlink);
            let (r, done) = buffered_text(ui, salt, &current, 0, "https://…");
            if let Err(error) = track_inspector_edit(
                d,
                ui,
                &r,
                crate::doc::InspectorField::Hyperlink,
                topic.hyperlink.clone().map(Into::into),
                done,
                false,
            ) {
                log::warn!("hyperlink draft: {error}");
                self.toasts.error(error);
            }
            let r = r.labelled_by(heading);
            if focus == Some(FocusField::Hyperlink) {
                if !r.has_focus() || ui.is_sizing_pass() || !ui.is_enabled() {
                    self.focus_field = focus;
                }
                if ui.is_enabled() && !ui.is_sizing_pass() {
                    r.request_focus();
                    r.scroll_to_me(Some(egui::Align::Center));
                }
            }
        });
        data_section(ui, t("Labels"), |ui, _| {
            ui.horizontal_wrapped(|ui| {
                for label in &topic.labels {
                    ui.label(label);
                }
            });
            edit_labels = ui
                .add_enabled(sel.len() == 1, egui::Button::new(t("Edit labels")))
                .clicked()
                && touch_allowed;
        });
        let current = topic.notes.as_deref().unwrap_or_default().to_owned();
        let markdown = topic.notes.as_ref().is_some_and(|note| note.is_markdown());
        let (r, mut done, mut format_notes) = data_section(ui, t("Notes"), |ui, heading| {
            let salt = (
                "notes",
                &d.key,
                d.doc.active_index(),
                d.doc.sheet().root,
                primary,
            );
            restore_inspector_buffer(d, ui, salt, primary, &crate::doc::InspectorField::Notes);
            let (r, mut done) = ui
                .add_enabled_ui(
                    !crate::notes::enlarged_active(ui.ctx())
                        && !crate::equations::enlarged_active(ui.ctx()),
                    |ui| buffered_text(ui, salt, &current, 5, ""),
                )
                .inner;
            let format_notes =
                crate::notes::tools(ui, &r, &current, &mut done, markdown, d.doc.state_id());
            let r = r.labelled_by(heading);
            if focus == Some(FocusField::Notes) {
                if !r.has_focus() || ui.is_sizing_pass() || !ui.is_enabled() {
                    self.focus_field = focus;
                }
                if ui.is_enabled() && !ui.is_sizing_pass() {
                    r.request_focus();
                    r.scroll_to_me(Some(egui::Align::Center));
                }
            }
            (r, done, format_notes)
        });
        if !crate::notes::enlarged_active(ui.ctx()) {
            format_notes |= data_section(ui, t("Equation"), |ui, _| {
                crate::notes::equation_tools(
                    ui,
                    &r,
                    &current,
                    &mut done,
                    markdown || format_notes,
                    d.doc.state_id(),
                )
            });
        }
        if let Err(error) = track_inspector_edit(
            d,
            ui,
            &r,
            crate::doc::InspectorField::Notes,
            topic.notes.clone(),
            done,
            format_notes,
        ) {
            log::warn!("notes draft: {error}");
            self.toasts.error(error);
        }

        if edit_labels {
            self.exec(ui.ctx(), Cmd::Labels);
        }
        data_divider(ui);
        self.task_properties(ui);
    }

    fn todo_properties(&mut self, ui: &mut Ui) {
        let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
        ui.separator();
        let todo = self.doc().and_then(|doc| {
            if doc.view.selection.len() != 1 {
                return None;
            }
            let id = doc.view.primary()?;
            doc.doc.sheet().topic(id)?.todo?;
            Some((id, *doc.doc.sheet().todo_progress().get(&id)?))
        });
        if let Some((id, progress)) = todo {
            ui.strong(t("To-Do"));
            ui.label(format!(
                "{}: {}/{}",
                t("Progress"),
                progress.completed,
                progress.total
            ));
            widgets::caption(
                ui,
                t("Counts deepest To-Dos only; scheduled Tasks stay unchanged."),
            );
            let mut change = None;
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !progress.complete(),
                        egui::Button::new(t("Complete subtree To-Dos")),
                    )
                    .clicked()
                    && touch_allowed
                {
                    change = Some(true);
                }
                if ui
                    .add_enabled(
                        progress.completed > 0,
                        egui::Button::new(t("Reopen subtree To-Dos")),
                    )
                    .clicked()
                    && touch_allowed
                {
                    change = Some(false);
                }
            });
            if let Some(done) = change
                && let Some(doc) = self.doc_mut()
            {
                let result = doc.flush_inspector_edit(ui.ctx()).and_then(|()| {
                    doc.view.commit_edit(&mut doc.doc);
                    doc.doc
                        .edit_sheet("To-Do", |sheet| sheet.set_todo_subtree(id, done))
                        .map_err(|error| error.to_string())
                });
                if let Err(error) = result {
                    log::warn!("To-Do subtree edit: {error}");
                    self.toasts.error(error);
                }
            }
        }
    }

    fn task_properties(&mut self, ui: &mut Ui) {
        let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
        if let Some(doc) = self.doc()
            && let Some(topic) = doc.view.primary()
            && let Some(task) = doc.doc.sheet().scheduling.tasks.get(&topic)
        {
            ui.strong(t("Task Planning"));
            ui.label(format!("{}: {}%", t("Progress"), task.progress_percent));
            match doc.doc.sheet().compute_schedule() {
                Ok(schedule) => {
                    if let Some(computed) = schedule.get(&topic) {
                        for (label, date) in [
                            ("Start date", computed.start_date),
                            ("Due date", computed.finish_date),
                        ] {
                            if let Some(date) = date {
                                ui.label(format!("{}: {date}", t(label)));
                            }
                        }
                    }
                }
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, error.to_string());
                }
            }
        }
        if ui.button(t("Task Planning")).clicked() && touch_allowed {
            self.exec(ui.ctx(), Cmd::TaskPlanning);
        }
    }

    fn relationship_style(
        &mut self,
        ui: &mut Ui,
        id: thinoda_core::RelationshipId,
        touch_allowed: bool,
    ) {
        let Some(d) = self.doc_mut() else { return };
        let Some(rel) = d
            .doc
            .sheet()
            .relationships
            .iter()
            .find(|r| r.id == id)
            .cloned()
        else {
            return;
        };
        ui.heading(t("Relationship"));
        ui.label(t(
            "Drag the round endpoints to reconnect. Drag square controls to reshape.",
        ));
        ui.label(t("Focused handles: arrows move controls by 1 map unit (Shift: 10); endpoint arrows cycle visible topics."));
        let salt = (d.key.clone(), d.doc.active_index(), id);
        let label = ui.label(t("Relationship title"));
        let (response, title) = buffered_text(ui, (&salt, "title"), &rel.title, 0, "");
        response.labelled_by(label.id);
        let mut from = rel.from;
        let mut to = rel.to;
        for (name, value, other) in [
            ("From topic", &mut from, rel.to),
            ("To topic", &mut to, rel.from),
        ] {
            widgets::property_field(ui, t(name), |ui| {
                egui::ComboBox::from_id_salt((&salt, name))
                    .selected_text(
                        d.doc
                            .sheet()
                            .topic(*value)
                            .map(|topic| topic.title.as_str())
                            .unwrap_or(""),
                    )
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for topic in d
                            .doc
                            .sheet()
                            .topics
                            .values()
                            .filter(|topic| topic.id != other)
                        {
                            ui.selectable_value(
                                value,
                                topic.id,
                                format!("{} ({})", topic.title, topic.id),
                            );
                        }
                    })
                    .response
            });
        }
        let mut color = None;
        widgets::property_row(ui, t("Line Color"), |ui| {
            color = widgets::color_swatches(ui, &SWATCHES, rel.color.map(from_color))
                .map(|color| Some(to_color(color)));
        });
        if rel.color.is_some() && ui.button(t("Automatic line color")).clicked() && touch_allowed {
            color = Some(None);
        }
        let layout = d.view.layout_for(&d.doc);
        let points = rel.control_points.or_else(|| {
            let curve = layout.relationships.iter().find(|r| r.id == id)?;
            Some([
                curve.c1 - layout.topics[&rel.from].rect.center(),
                curve.c2 - layout.topics[&rel.to].rect.center(),
            ])
        });
        ui.label(t(
            "Curve coordinates are offsets from each endpoint topic center.",
        ));
        let mut changed_points = None;
        let mut parse_error = None;
        if let Some(mut points) = points {
            let mut edited = false;
            for (index, name) in [
                "First control X",
                "First control Y",
                "Second control X",
                "Second control Y",
            ]
            .into_iter()
            .enumerate()
            {
                let point = &mut points[index / 2];
                let value = if index % 2 == 0 {
                    &mut point.x
                } else {
                    &mut point.y
                };
                widgets::property_field(ui, t(name), |ui| {
                    let (response, done) =
                        buffered_text(ui, (&salt, index), &value.to_string(), 0, "");
                    if let Some(done) = done {
                        match done.trim().parse::<f32>() {
                            Ok(next) => {
                                *value = next;
                                edited = true;
                            }
                            Err(e) => parse_error = Some(e.to_string()),
                        }
                    }
                    response
                });
            }
            if edited {
                changed_points = Some(points);
            }
        } else {
            widgets::caption(
                ui,
                t("Expand both endpoints to edit automatic curve controls."),
            );
        }
        let reset = ui
            .add_enabled(
                rel.control_points.is_some(),
                egui::Button::new(t("Reset curve")),
            )
            .clicked()
            && touch_allowed;
        let reconnect = touch_allowed && (from != rel.from || to != rel.to);
        let changed =
            title.is_some() || color.is_some() || reset || changed_points.is_some() || reconnect;
        let result = if changed {
            d.doc.edit_sheet("Edit Relationship", |s| {
                if let Some(title) = title {
                    s.set_relationship_title(id, &title)?;
                }
                if reconnect {
                    s.reconnect_relationship(id, from, to)?;
                }
                if let Some(color) = color {
                    s.set_relationship_color(id, color)?;
                }
                if reset {
                    s.set_relationship_control_points(id, None)?;
                } else if let Some(points) = changed_points {
                    s.set_relationship_control_points(id, Some(points))?;
                }
                Ok(())
            })
        } else {
            Ok(())
        };
        if let Err(e) = result {
            self.toasts
                .warning(format!("{}: {e}", t("Not possible here")));
        }
        if let Some(e) = parse_error {
            self.toasts
                .warning(format!("{}: {e}", t("Invalid curve coordinate")));
        }
    }

    fn map_tab(&mut self, ui: &mut Ui) {
        let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
        self.custom_theme_gallery(ui);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        let sheet = d.doc.sheet();
        let mut ts = sheet.theme.clone();
        let structure = sheet.structure;
        let before = ts.clone();
        widgets::caption(ui, t("Map settings apply to the entire sheet."));

        widgets::section(ui, t("Structure"), true, |ui| {
            let (_, chosen) = crate::structure_picker::show(ui, structure);
            if touch_allowed && let Some(s) = chosen.filter(|&s| s != structure) {
                d.view.commit_edit(&mut d.doc);
                match d
                    .doc
                    .edit_sheet("Structure", |sh| sh.set_structure(None, Some(s)))
                {
                    Ok(()) => d.view.request_fit(),
                    Err(error) => {
                        self.toasts.error(error.to_string());
                    }
                }
            }
        });

        widgets::section(ui, t("Theme"), true, |ui| {
            widgets::property_field(ui, t("Global Font"), |ui| {
                font_selector(ui, &mut self.fonts, &mut ts.font_family, t("Default"))
            });
            let width = ui.available_width();
            let columns = ((width + 8.0) / 126.0).floor().max(1.0) as usize;
            let card_width =
                ((width - 8.0 * (columns - 1) as f32) / columns as f32).clamp(1.0, 118.0);
            egui::Grid::new("sheet_themes")
                .num_columns(columns)
                .spacing(vec2(8.0, 8.0))
                .show(ui, |ui| {
                    for (index, id) in Theme::IDS.into_iter().enumerate() {
                        let th = Theme::builtin(id);
                        let selected = ts.custom_theme.is_none() && ts.theme_id == id;
                        let (_, activated) = widgets::preview_card(
                            ui,
                            t(th.name),
                            selected,
                            card_width,
                            38.0,
                            |ui, rect| {
                                let painter = ui.painter();
                                painter.rect_filled(
                                    rect,
                                    CornerRadius::same(4),
                                    from_color(th.light_bg),
                                );
                                let c = rect.center();
                                painter.rect_filled(
                                    egui::Rect::from_center_size(c, vec2(30.0, 14.0)),
                                    CornerRadius::same(3),
                                    from_color(th.central_fill),
                                );
                                for (k, col) in th.palette.iter().take(4).enumerate() {
                                    let x = if k % 2 == 0 {
                                        rect.left() + 14.0
                                    } else {
                                        rect.right() - 14.0
                                    };
                                    let y = if k < 2 {
                                        rect.top() + 9.0
                                    } else {
                                        rect.bottom() - 9.0
                                    };
                                    painter.rect_filled(
                                        egui::Rect::from_center_size(
                                            egui::pos2(x, y),
                                            vec2(18.0, 7.0),
                                        ),
                                        CornerRadius::same(2),
                                        from_color(*col),
                                    );
                                }
                            },
                        );
                        if activated {
                            ts.theme_id = id.to_string();
                            ts.custom_theme = None;
                            ts.variant = 0;
                        }
                        if (index + 1) % columns == 0 {
                            ui.end_row();
                        }
                    }
                });
            let th = Theme::for_settings(&ts);
            widgets::property_field(ui, t("Color Variant"), |ui| {
                egui::ComboBox::from_id_salt("variant")
                    .selected_text(if ts.variant == 0 {
                        t("Multicolor").to_string()
                    } else {
                        format!("#{}", ts.variant)
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut ts.variant, 0, t("Multicolor"));
                        for v in 1..th.variant_count() {
                            ui.selectable_value(&mut ts.variant, v, format!("#{v}"));
                        }
                    })
                    .response
            });
            widgets::property_field(ui, t("Rainbow Branches"), |ui| {
                widgets::toggle(ui, &mut ts.rainbow, t("Rainbow Branches"))
            });
            widgets::property_field(ui, t("Tapered Lines"), |ui| {
                widgets::toggle(ui, &mut ts.tapered_lines, t("Tapered Lines"))
            });
            widgets::property_field(ui, t("Balanced Map"), |ui| {
                widgets::toggle(ui, &mut ts.balanced, t("Balanced Map"))
            });
        });

        widgets::section(ui, t("Branches"), true, |ui| {
            let mut automatic = ts.line_width.is_none();
            widgets::property_field(ui, t("Automatic branch width"), |ui| {
                widgets::toggle(ui, &mut automatic, t("Automatic branch width"))
            });
            if automatic {
                ts.line_width = None;
            } else {
                let width = ts.line_width.unwrap_or(1.5);
                ts.line_width = Some(width);
                widgets::property_field(ui, t("Branch Line Width"), |ui| {
                    let (response, done) = buffered_text(
                        ui,
                        (
                            &d.key,
                            d.doc.active_index(),
                            d.doc.state_id(),
                            "branch-width",
                        ),
                        &width.to_string(),
                        0,
                        "0.5-16",
                    );
                    if let Some(done) = done {
                        match done.trim().parse::<f32>() {
                            Ok(value) => ts.line_width = Some(value),
                            Err(error) => {
                                self.toasts
                                    .warning(format!("{}: {error}", t("Invalid branch width")));
                            }
                        }
                    }
                    response
                });
            }
        });

        widgets::section(ui, t("Advanced Layout"), true, |ui| {
            widgets::property_field(ui, t("Flexible Floating Topics"), |ui| {
                widgets::toggle(
                    ui,
                    &mut ts.flexible_floating_topics,
                    t("Flexible Floating Topics"),
                )
            });
            widgets::caption(
                ui,
                t(
                    "Place floating topics without attaching. Turn off to convert between attached and floating topics.",
                ),
            );
        });

        widgets::section(ui, t("Background"), true, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(ts.background.is_none(), t("Auto"))
                    .clicked()
                {
                    ts.background = None;
                }
            });
            let background = Theme::for_settings(&ts).background(&ts, d.view.dark);
            if let Some(c) = widgets::color_swatches(ui, &SWATCHES, Some(from_color(background))) {
                ts.background = Some(to_color(c));
            }
        });

        if touch_allowed && ts != before {
            let result = d.doc.edit_sheet(t("Theme"), |s| {
                ts.validate().map_err(thinoda_core::EditError::Invalid)?;
                s.theme = ts;
                Ok(())
            });
            if let Err(error) = result {
                self.toasts.error(error.to_string());
            }
        }
    }

    // ----- search bar ---------------------------------------------------------------------------

    fn search_bar(&mut self, ctx: &Context) {
        let mut close = false;
        let touch_allowed = widgets::touch_activation_allowed(ctx);
        let response = Dialog::new(egui::Id::unique("thinoda_search"), t("Find and Replace…"))
            .icon(icons::SEARCH)
            .labels(&i18n::dialog_labels())
            .default_width(520.0)
            .default_height(420.0)
            .min_size(egui::vec2(280.0, 180.0))
            .fit_content_height(true)
            .show(ctx, |ui| {
                            let r = widgets::search_field(ui, &mut self.search.query, t("Search"));
                            if self.search.focus && ui.is_enabled() && !ui.is_sizing_pass() {
                                self.search.focus = !r.has_focus();
                                r.request_focus();
                            }
                            if r.lost_focus()
                                && ui.input(|i| i.key_pressed(Key::Enter))
                                && touch_allowed
                            {
                                let back = ui.input(|i| i.modifiers.shift);
                                self.goto_hit(if back { -1 } else { 1 });
                                r.request_focus();
                            }
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .small_button(icons::ARROW_UP)
                                    .accessible_name(t("Previous result"))
                                    .clicked()
                                    && touch_allowed
                                {
                                    self.goto_hit(-1);
                                }
                                if ui
                                    .small_button(icons::ARROW_DOWN)
                                    .accessible_name(t("Next result"))
                                    .clicked()
                                    && touch_allowed
                                {
                                    self.goto_hit(1);
                                }
                                if ui
                                    .small_button(icons::CLOSE)
                                    .accessible_name(t("Close search"))
                                    .clicked()
                                    && touch_allowed
                                {
                                    close = true;
                                }
                            });
                            let n = self.search.result_count();
                            if !self.search.query.is_empty() || self.search.facets.active() {
                            widgets::caption(
                                ui,
                                if n == 0 {
                                    t("No results").to_string()
                                } else {
                                    format!("{}/{} {}", self.search.current + 1, n, t("results"))
                                },
                            );
                            }
                            if self.search.all_sheets
                                && let Some(index) = self.search.current_sheet()
                                && let Some(sheet) = self
                                    .doc()
                                    .and_then(|d| d.doc.workbook().sheets.get(index))
                            {
                                let title = format!("{}: {}", t("Sheet"), sheet.title);
                                ui.add(egui::Label::new(&title).truncate())
                                    .on_hover_text(title);
                            }
                            let options = (
                                self.search.all_sheets,
                                self.search.opts.clone(),
                                self.search.dim,
                                self.search.show_replace,
                            );
                            ui.vertical(|ui| {
                                ui.checkbox(&mut self.search.all_sheets, t("All sheets"));
                                ui.checkbox(&mut self.search.opts.case_sensitive, t("Match case"));
                                ui.checkbox(&mut self.search.opts.whole_word, t("Whole word"));
                                ui.checkbox(
                                    &mut self.search.opts.include_notes,
                                    t("Include notes"),
                                );
                                ui.checkbox(
                                    &mut self.search.opts.include_labels,
                                    t("Include labels"),
                                );
                                ui.checkbox(&mut self.search.dim, t("Dim others"));
                                ui.checkbox(&mut self.search.show_replace, t("Replace"));
                            });
                            if !touch_allowed {
                                (
                                    self.search.all_sheets,
                                    self.search.opts,
                                    self.search.dim,
                                    self.search.show_replace,
                                ) = options;
                            }
                            if self.search.opts.include_notes {
                                widgets::caption(
                                    ui,
                                    t("Notes search uses visible Markdown text; oversized notes search the original source."),
                                );
                            }
                            if self.search.show_replace {
                                ui.separator();
                                ui.label(t("Replace with"));
                                widgets::caption(
                                    ui,
                                    t("Replaces topic titles, board labels and descriptions."),
                                );
                                ui.add(
                                    TextEdit::singleline(&mut self.search.replace)
                                        .hint_text(t("Replace"))
                                        .desired_width(ui.available_width()),
                                )
                                .accessible_name(t("Replace with"));
                                ui.horizontal_wrapped(|ui| {
                                    if ui
                                        .add_enabled(
                                            n > 0 && !self.search.facets.notes_only,
                                            egui::Button::new(t("Replace All")),
                                        )
                                        .clicked()
                                        && touch_allowed
                                    {
                                        self.replace_all();
                                    }
                                });
                            }
                            self.search_facets_ui(ui, touch_allowed);
                            reveal_changed_focus(ui);
            });
        if close || response.close_requested {
            self.search.open = false;
        }
        self.apply_search_filter();
    }

    // ----- dialogs ------------------------------------------------------------------------------

    fn dialogs_ui(&mut self, ctx: &Context) {
        if self.board_preflight_ui(ctx) {
            return;
        }
        if self.drawio_link_ui(ctx) {
            return;
        }
        self.cloud_change_dialogs(ctx);
        self.show_cloud_dialogs(ctx);
        if self.fim_dialogs(ctx) {
            return;
        }
        if self.dialogs.symbol_export.is_some()
            || self.dialogs.note_export.is_some()
            || self.dialogs.mindmanager_export
            || self.dialogs.powerpoint_export
        {
            let valid = self.doc().is_some_and(|doc| {
                self.dialogs.export_owner.as_ref()
                    == Some(&(doc.key.clone(), doc.doc.active_index(), doc.doc.state_id()))
            });
            if !valid {
                self.dialogs.symbol_export = None;
                self.dialogs.note_export = None;
                self.dialogs.mindmanager_export = false;
                self.dialogs.powerpoint_export = false;
                self.dialogs.mindmanager_symbol_color_loss = false;
                self.dialogs.export_owner = None;
                self.toasts.warning(t("Not possible here"));
                return;
            }
        }
        if let Some((format, allow_symbol_color_loss)) = self.dialogs.note_export {
            if let Some(choice) = confirm_dialog(
                ctx,
                "note_export",
                t("Export note source"),
                t(crate::notes::EXPORT_SOURCE_WARNING),
                &[
                    (t("Export"), DialogButton::Primary),
                    (t("Cancel"), DialogButton::Secondary),
                ],
                &i18n::dialog_labels(),
            ) {
                self.dialogs.note_export = None;
                self.dialogs.export_owner = None;
                if choice == 0 {
                    self.export_with_note_consent(format, allow_symbol_color_loss, true);
                }
            }
            return;
        }
        if let Some((_, format)) = self.dialogs.symbol_export {
            if let Some(choice) = confirm_dialog(
                ctx,
                "symbol_export",
                t("Symbol color"),
                t(crate::symbols::EXPORT_WARNING),
                &[
                    (t("Export"), DialogButton::Primary),
                    (t("Cancel"), DialogButton::Secondary),
                ],
                &i18n::dialog_labels(),
            ) {
                self.dialogs.symbol_export = None;
                self.dialogs.export_owner = None;
                if choice == 0 {
                    self.export(format, true);
                }
            }
            return;
        }
        if self.dialogs.mindmanager_save.is_some() {
            let message = format!(
                "{}\n\n{}",
                t(
                    "MindManager saves one sheet. Export the current sheet or save the complete workbook as .thinoda."
                ),
                t(
                    "Theme colors and appearance may differ. Use .thinoda to preserve all Thinoda data."
                )
            );
            if let Some(choice) = confirm_dialog(
                ctx,
                "mindmanager_save",
                t("Save As…"),
                &message,
                &[
                    (t("Save"), DialogButton::Primary),
                    (t("Cancel"), DialogButton::Secondary),
                ],
                &i18n::dialog_labels(),
            ) {
                let (key, path, sheet_only) = self.dialogs.mindmanager_save.take().unwrap();
                let deliver = std::mem::take(&mut self.dialogs.mindmanager_save_deliver);
                let index = self.docs.iter().position(|doc| doc.key == key);
                let saved = if choice == 0 {
                    if let Some(index) = index {
                        self.active = index;
                        self.request_save_preflight(
                            &path,
                            self.settings.backup_on_save,
                            deliver,
                            sheet_only,
                            true,
                        )
                    } else {
                        self.toasts.warning(t("Not possible here"));
                        false
                    }
                } else {
                    if let Some(index) = index {
                        self.docs[index].ignore_conflict = false;
                    }
                    false
                };
                if self.resolve_save_all(ctx, &key, saved) {
                    return;
                }
                if !sheet_only {
                    self.resolve_save_close_tab(&key, saved);
                }
            }
            return;
        }
        if self.dialogs.mindmanager_export {
            if let Some(choice) = confirm_dialog(
                ctx,
                "mindmanager_export",
                t("Export MindManager copy"),
                t(
                    "Only the current sheet is exported. Colors, assets, relationships, advanced layout and unsupported data may be omitted. The source workbook is unchanged.",
                ),
                &[
                    (t("Export"), DialogButton::Primary),
                    (t("Cancel"), DialogButton::Secondary),
                ],
                &i18n::dialog_labels(),
            ) {
                self.dialogs.mindmanager_export = false;
                self.dialogs.export_owner = None;
                let allow_symbol_color_loss =
                    std::mem::take(&mut self.dialogs.mindmanager_symbol_color_loss);
                if choice == 0 {
                    self.export_confirmed(thinoda_io::Format::MindManager, allow_symbol_color_loss);
                }
            }
            return;
        }
        if self.dialogs.powerpoint_export {
            let owner = self.dialogs.export_owner.clone().expect("validated owner");
            let mut export = false;
            let mut cancel = false;
            let mut can_export = false;
            let ratio = &mut self.dialogs.powerpoint_aspect_ratio;
            let preview = &mut self.dialogs.powerpoint_preview;
            let response = Dialog::new(
                egui::Id::unique("powerpoint_export"),
                t("PowerPoint slide size"),
            )
            .labels(&i18n::dialog_labels())
            .modal(true)
            .close_on_backdrop(true)
            .default_width(620.0)
            .default_height(540.0)
            .fit_content_height(true)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(t(
                        "Slides use editable text, vector shapes and paths. Photos remain images. The frame expands to the selected ratio without cropping or stretching.",
                    ))
                    .color(Palette::of_ui(ui).text_muted),
                );
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.radio_value(
                        ratio,
                        thinoda_io::PresentationAspectRatio::Widescreen16x9,
                        t("Widescreen (16:9)"),
                    );
                    ui.radio_value(
                        ratio,
                        thinoda_io::PresentationAspectRatio::Standard4x3,
                        t("Standard (4:3)"),
                    );
                });
                ui.add_space(8.0);
                let source = self
                    .docs
                    .iter()
                    .find(|doc| doc.key == owner.0)
                    .filter(|doc| {
                        doc.doc.active_index() == owner.1 && doc.doc.state_id() == owner.2
                    });
                if let Some(document) = source {
                    let sheet = document.doc.sheet();
                    if sheet.presentation.slides.is_empty() {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            t("No slides yet. Generate branch and Zone slides, or add the selected branch."),
                        );
                    } else {
                        can_export = true;
                        ui.label(t("Slide preview"));
                        let width = ui.available_width().min(560.0);
                        preview.show(
                            ui,
                            sheet,
                            &owner,
                            *ratio,
                            vec2(width, (width * 9.0 / 16.0).clamp(140.0, 315.0)),
                        );
                    }
                } else {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        t("The document changed. Try again."),
                    );
                }
                ui.add_space(8.0);
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), ui.spacing().interact_size.y.max(28.0)),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let export_response = ui
                            .add_enabled_ui(can_export, |ui| {
                                thinoda_ui::widgets::primary_button(ui, t("Export"))
                            })
                            .inner;
                        if export_response.clicked()
                            && thinoda_ui::widgets::touch_activation_allowed(ui.ctx())
                        {
                            export = true;
                        }
                        if thinoda_ui::widgets::secondary_button(ui, t("Cancel")).clicked()
                            && thinoda_ui::widgets::touch_activation_allowed(ui.ctx())
                        {
                            cancel = true;
                        }
                    },
                );
            });
            if response.close_requested || cancel {
                self.dialogs.powerpoint_export = false;
                self.dialogs.export_owner = None;
            } else if export {
                self.dialogs.powerpoint_export = false;
                self.dialogs.export_owner = None;
                self.export_confirmed(thinoda_io::Format::PowerPoint, false);
            }
            return;
        }
        if let Some((path, document, warnings, owner)) = self.dialogs.imports.front() {
            if owner.is_some_and(|owner| owner != self.current_viewport) {
                return;
            }
            let (source, notice) = match path {
                Some(path) => (
                    path.display().to_string(),
                    t("This import may omit unsupported content. The source file is preserved."),
                ),
                None => (
                    t(crate::drawio_link::LINK_LABEL).into(),
                    t(crate::drawio_link::SOURCE_NOTICE),
                ),
            };
            let mut message = format!("{}\n\n{}", source, notice);
            let sheet = document.doc.sheet();
            let preview_text = |text: &str| -> String {
                text.chars()
                    .map(|character| {
                        if character.is_control() {
                            ' '
                        } else {
                            character
                        }
                    })
                    .take(120)
                    .collect()
            };
            if sheet.topic_count() > 0 {
                message.push_str(&format!(
                    "\n\n{} ({}; {}):",
                    t("Converted outline preview"),
                    sheet.topic_count(),
                    t("showing the first 8 topics")
                ));
                let mut preview: Vec<_> = if sheet.is_anchor(sheet.root) {
                    sheet
                        .content_top_level_ids()
                        .into_iter()
                        .rev()
                        .map(|id| (id, 0usize))
                        .collect()
                } else {
                    vec![(sheet.root, 0usize)]
                };
                for _ in 0..8 {
                    let Some((id, depth)) = preview.pop() else {
                        break;
                    };
                    let topic = &sheet.topics[&id];
                    message.push_str(&format!(
                        "\n{}- {}",
                        "  ".repeat(depth.min(12)),
                        preview_text(&topic.title)
                    ));
                    preview.extend(topic.children.iter().rev().map(|child| (*child, depth + 1)));
                }
            }
            let labels: Vec<_> = sheet
                .canvas
                .objects
                .iter()
                .map(|object| object.label())
                .filter(|label| !label.trim().is_empty())
                .take(8)
                .collect();
            if !labels.is_empty() {
                message.push_str(&format!("\n\n{}:", t("Converted drawing labels (up to 8)")));
                for label in labels {
                    message.push_str(&format!("\n- {}", preview_text(label)));
                }
            }
            message.push_str(&format!("\n\n{}:", t("Conversion warnings")));
            for warning in warnings {
                let label = match warning {
                    thinoda_io::ImportWarning::Formatting => "Formatting and styles",
                    thinoda_io::ImportWarning::Notes => "Rich notes",
                    thinoda_io::ImportWarning::Resources => "Images and attachments",
                    thinoda_io::ImportWarning::Markers => "Markers and task data",
                    thinoda_io::ImportWarning::Links => "Additional or unsupported links",
                    thinoda_io::ImportWarning::Other => "Other unsupported map data",
                };
                message.push_str("\n- ");
                message.push_str(t(label));
            }
            if let Some(choice) = confirm_dialog(
                ctx,
                "document_import_confirmation",
                t("Import requires confirmation"),
                &message,
                &[
                    (t("Import converted copy"), DialogButton::Primary),
                    (t("Cancel"), DialogButton::Secondary),
                ],
                &i18n::dialog_labels(),
            ) {
                self.resolve_import(choice == 0);
            }
            return;
        }
        // Crash recovery.
        if !self.dialogs.recovery.is_empty() {
            enum RecoveryAction {
                Restore(Option<usize>),
                Discard,
                Keep,
                #[cfg(target_arch = "wasm32")]
                Download(thinoda_platform::recovery::OrphanDoc),
            }
            let touch_allowed = widgets::touch_activation_allowed(ctx);
            let mut action = None;
            let labels = i18n::dialog_labels();
            thinoda_ui::Dialog::new(
                egui::Id::unique("thinoda_recovery"),
                t("Recover Unsaved Work"),
            )
            .icon(icons::RESTORE)
            .labels(&labels)
            .modal(true)
            .close_on_backdrop(true)
            .default_width(460.0)
            .default_height(500.0)
            .min_size(egui::vec2(340.0, 220.0))
            .show(ctx, |ui| {
                ui.label(t(if self.recovery_discovery_incomplete {
                    "Some recovery data could not be checked. Keep existing storage and save your work."
                } else {
                    "Thinoda was not closed properly. The following documents can be restored:"
                }));
                #[cfg(target_arch = "wasm32")]
                if self.recovery_discovery_incomplete {
                    ui.label(t("Raw downloads contain a browser storage record (UTF-16LE code units, no BOM), not a .thinoda document. The original stays in browser storage."));
                }
                ui.add_space(8.0);
                for (i, s) in self.dialogs.recovery.iter().enumerate() {
                    for doc in &s.docs {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&doc.meta.title).strong());
                            if let Some(p) = &doc.meta.original_path {
                                widgets::caption(ui, p.display().to_string());
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button(t("Restore")).clicked() && touch_allowed {
                                    action = Some(RecoveryAction::Restore(Some(i)));
                                }
                            });
                        });
                        #[cfg(target_arch = "wasm32")]
                        if self.recovery_discovery_incomplete
                            && ui.button(t("Download raw record")).clicked()
                            && touch_allowed
                        {
                            action = Some(RecoveryAction::Download(doc.clone()));
                        }
                    }
                }
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if widgets::primary_button(ui, t("Restore All")).clicked() && touch_allowed {
                        action = Some(RecoveryAction::Restore(None));
                    }
                    if self.recovery_discovery_incomplete {
                        if widgets::secondary_button(ui, t("Keep recovery data")).clicked()
                            && touch_allowed
                        {
                            action = Some(RecoveryAction::Keep);
                        }
                    } else if widgets::secondary_button(ui, t("Discard All")).clicked()
                        && touch_allowed
                    {
                        action = Some(RecoveryAction::Discard);
                    }
                });
            });
            match action {
                Some(RecoveryAction::Restore(which)) => self.restore_orphans(which),
                Some(RecoveryAction::Discard) => self.discard_orphans(),
                Some(RecoveryAction::Keep) => self.dialogs.recovery.clear(),
                #[cfg(target_arch = "wasm32")]
                Some(RecoveryAction::Download(doc)) => self.download_raw_recovery(&doc),
                None => {}
            }
            #[cfg(target_arch = "wasm32")]
            if self.dialogs.recovery.is_empty() {
                crate::app::set_browser_recovery_announcement("thinoda-recovery-download", None);
            }
        }

        // Save changes?
        if let Some(intent) = self.dialogs.close.clone() {
            let buttons = [
                (t("Save"), DialogButton::Primary),
                (t("Don't Save"), DialogButton::Danger),
                (t("Cancel"), DialogButton::Secondary),
            ];
            if let Some(b) = confirm_dialog(
                ctx,
                "thinoda_close",
                t("Save changes?"),
                t(
                    "Do you want to save the changes you made? Your changes will be lost if you don't save them.",
                ),
                &buttons,
                &i18n::dialog_labels(),
            ) {
                self.dialogs.close = None;
                match (intent, b) {
                    (CloseIntent::Tab(key), 0) => {
                        if let Some(index) = self.docs.iter().position(|doc| doc.key == key) {
                            self.active = index;
                            self.dialogs.save_close_tab = Some(key.clone());
                            let saved = self.save_active(false);
                            self.resolve_save_close_tab(&key, saved);
                        } else {
                            self.toasts.warning(t("Not possible here"));
                        }
                    }
                    (CloseIntent::Tab(key), 1) => {
                        if let Some(index) = self.docs.iter().position(|doc| doc.key == key) {
                            self.close_tab_now(index);
                        } else {
                            self.toasts.warning(t("Not possible here"));
                        }
                    }
                    (intent @ (CloseIntent::Window | CloseIntent::Quit), 0) => {
                        self.begin_save_all(ctx, Some(intent));
                    }
                    (intent @ (CloseIntent::Window | CloseIntent::Quit), 1) => {
                        if let Some(index) =
                            self.docs.iter().position(|doc| doc.has_unresolved_ink())
                        {
                            self.focus_pending_ink(ctx, index);
                        } else {
                            for d in &mut self.docs {
                                d.doc.mark_saved();
                            }
                            self.approved_close = Some(intent);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    _ => {}
                }
            }
        }

        // External modification conflict.
        if !self.cloud_changes.comparing()
            && let Some(key) = self.dialogs.conflict.clone()
        {
            let buttons = [
                (t("Overwrite"), DialogButton::Danger),
                (t("Save As Copy"), DialogButton::Primary),
                (t("Compare"), DialogButton::Secondary),
                (t("Cancel"), DialogButton::Secondary),
            ];
            if let Some(b) = confirm_dialog(
                ctx,
                "thinoda_conflict",
                t("File changed on disk"),
                t("The file was modified by another application or a sync client. Overwrite it?"),
                &buttons,
                &i18n::dialog_labels(),
            ) {
                self.dialogs.conflict = None;
                let conflict_target = self
                    .dialogs
                    .conflict_save_target
                    .take()
                    .filter(|(owner, _)| *owner == key);
                let saved = if let Some(index) = self.docs.iter().position(|doc| doc.key == key) {
                    self.active = index;
                    match b {
                        0 => {
                            self.docs[index].ignore_conflict = true;
                            if let Some((_, target)) = conflict_target {
                                self.save_active_to(&target, self.settings.backup_on_save)
                            } else {
                                self.save_active(false)
                            }
                        }
                        1 => self.save_active(true),
                        2 => {
                            if let Some(path) = conflict_target
                                .as_ref()
                                .map(|(_, path)| path.clone())
                                .or_else(|| self.docs[index].path.clone())
                            {
                                self.compare_file(&key, &path);
                                self.dialogs.conflict = Some(key.clone());
                                self.dialogs.conflict_save_target = conflict_target;
                            }
                            false
                        }
                        _ => false,
                    }
                } else {
                    self.toasts.warning(t("Not possible here"));
                    false
                };
                if !self.resolve_save_all(ctx, &key, saved) {
                    self.resolve_save_close_tab(&key, saved);
                }
            }
        }

        // Rename sheet.
        if let Some(mut draft) = self.dialogs.rename_sheet.take() {
            let valid = self.doc().is_some_and(|doc| {
                doc.key == draft.document_key
                    && doc.doc.state_id() == draft.revision
                    && doc.doc.active_index() == draft.index
            });
            if !valid {
                self.toasts.warning(t("Not possible here"));
                return;
            }
            let touch_allowed = widgets::touch_activation_allowed(ctx);
            let mut done = None;
            let edit_id = egui::Id::unique("thinoda_rename_input");
            let enter = ctx.memory(|memory| memory.has_focus(edit_id))
                && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter));
            let labels = i18n::dialog_labels();
            let response =
                thinoda_ui::Dialog::new(egui::Id::unique("thinoda_rename"), t("Rename Sheet"))
                    .icon(icons::SHEET)
                    .labels(&labels)
                    .modal(true)
                    .close_on_backdrop(true)
                    .default_width(360.0)
                    .default_height(150.0)
                    .min_size(egui::vec2(320.0, 120.0))
                    .resizable(false)
                    .show(ctx, |ui| {
                        let r = ui
                            .add(
                                TextEdit::singleline(&mut draft.name)
                                    .id(edit_id)
                                    .lock_focus(draft.ime_composing)
                                    .desired_width(280.0),
                            )
                            .accessible_name(t("Rename Sheet"));
                        if !draft.focus_requested {
                            r.request_focus();
                            draft.focus_requested = true;
                        }
                        if !r.has_focus() && !r.lost_focus() {
                            draft.ime_composing = false;
                        }
                        ui.horizontal(|ui| {
                            if (widgets::primary_button(ui, t("OK")).clicked() || enter)
                                && touch_allowed
                            {
                                done = Some(true);
                            }
                            if widgets::secondary_button(ui, t("Cancel")).clicked() && touch_allowed
                            {
                                done = Some(false);
                            }
                        });
                    });
            if response.close_requested && touch_allowed {
                done = Some(false);
            }
            match done {
                Some(true) => {
                    if let Some(d) = self.docs.get_mut(self.active) {
                        let prev = d.doc.active_index();
                        d.doc.set_active(draft.index);
                        let n = draft.name;
                        let result = d.doc.edit_sheet("Rename Sheet", |s| {
                            s.title = n;
                            Ok(())
                        });
                        d.doc.set_active(prev);
                        if let Err(error) = result {
                            self.toasts.error(error.to_string());
                        }
                    }
                }
                Some(false) => {}
                None => self.dialogs.rename_sheet = Some(draft),
            }
        }

        if self.dialogs.settings {
            self.settings_window(ctx);
        } else {
            self.dialogs.settings_gesture = None;
        }
        if self.dialogs.about {
            let touch_allowed = widgets::touch_activation_allowed(ctx);
            let labels = crate::i18n::dialog_labels();
            let response = Dialog::new(egui::Id::unique("thinoda_about"), t("About Thinoda"))
                .icon(icons::INFO)
                .labels(&labels)
                .default_width(420.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        crate::brand::logo(ui, 72.0);
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.heading(RichText::new("Thinoda").size(26.0).strong());
                            ui.label(RichText::new(t(crate::brand::TAGLINE)).italics());
                        });
                    });
                    ui.add_space(6.0);
                    ui.label(t("A fast, open mind mapping app."));
                    ui.label(format!("{} Axel Wolters", t("Written by")));
                    if ui
                        .link(format!("{}: {}", t("Contact"), crate::brand::CONTACT_EMAIL))
                        .on_hover_text(t("Write an email"))
                        .clicked()
                        && touch_allowed
                    {
                        ctx.open_url(egui::OpenUrl::new_tab(format!(
                            "mailto:{}",
                            crate::brand::CONTACT_EMAIL
                        )));
                    }
                    ui.label(format!(
                        "{} {} · {} {}",
                        t("Version"),
                        thinoda_platform::system::app_version(),
                        thinoda_platform::system::os_name(),
                        thinoda_platform::system::arch()
                    ));
                    if ui
                        .link(crate::brand::PROJECT_URL)
                        .on_hover_text(t("Open in web browser"))
                        .clicked()
                        && touch_allowed
                    {
                        ctx.open_url(egui::OpenUrl::new_tab(crate::brand::PROJECT_URL));
                    }
                    ui.separator();
                    if ui.button(t("Legal documents")).clicked() && touch_allowed {
                        self.dialogs.legal.open = true;
                    }
                });
            self.dialogs.about = !response.close_requested || !touch_allowed;
        }
        if self.dialogs.masked_copy {
            let source = self
                .doc()
                .map(|d| (d.key.clone(), d.doc.active_index(), d.doc.state_id()));
            if source.is_none() || source != self.dialogs.masked_copy_owner {
                self.dialogs.masked_copy = false;
                self.dialogs.masked_copy_owner = None;
                self.toasts.warning(t("The document changed. Try again."));
                return;
            }
            let mut create = false;
            let mut cancel = false;
            let touch_allowed = widgets::touch_activation_allowed(ctx);
            let labels = crate::i18n::dialog_labels();
            let response = Dialog::new(egui::Id::unique("thinoda_masked_copy"), t("Create Masked Copy…"))
                .icon(icons::MASK)
                .labels(&labels)
                .default_width(460.0)
                .show(ctx, |ui| {
                    ui.label(t("Creates a new unsaved document. The original is not changed."));
                    ui.label(t("Replaces all topic, sheet and relationship titles. Removes notes, labels, links, images, markers, custom font names, boundary titles and document metadata."));
                    ui.label(t("Structure, colors and task states remain visible. This does not guarantee anonymity; inspect every sheet before sharing."));
                    ui.horizontal(|ui| {
                        if ui.button(t("Cancel")).clicked() && touch_allowed { cancel = true; }
                        if ui.button(t("Create Preview")).clicked() && touch_allowed { create = true; }
                    });
                });
            self.dialogs.masked_copy = (!response.close_requested || !touch_allowed) && !cancel;
            if !self.dialogs.masked_copy {
                self.dialogs.masked_copy_owner = None;
            }
            if create {
                self.create_masked_copy();
            }
        }
    }

    fn settings_window(&mut self, ctx: &Context) {
        let mut open = true;
        let before = self.settings.clone();
        let touch_allowed = widgets::touch_activation_allowed(ctx);
        let frame_number = ctx.cumulative_frame_nr();
        let (pointer_down, focused) = ctx.input(|input| {
            (
                input.pointer.primary_down(),
                input.focused
                    && !input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::WindowFocused(false))),
            )
        });
        if pointer_down && self.dialogs.settings_gesture.is_none() {
            self.dialogs.settings_gesture = Some(SettingsGesture {
                settings: before.clone(),
                accent: self.accent,
                staged: before.clone(),
                staged_accent: self.accent,
                frame: frame_number,
                canceled: false,
            });
        }
        if let Some(gesture) = &mut self.dialogs.settings_gesture {
            gesture.canceled |= !touch_allowed
                || !focused
                || frame_number > gesture.frame.saturating_add(1)
                || self.dialogs.shortcuts
                || self.settings != gesture.settings
                || self.accent != gesture.accent;
            gesture.frame = frame_number;
        }
        let mut activation_allowed = touch_allowed
            && focused
            && self
                .dialogs
                .settings_gesture
                .as_ref()
                .is_none_or(|gesture| !gesture.canceled);
        let before_accent = self.accent;
        let before_tab = self.dialogs.settings_tab;
        if let Some(gesture) = &self.dialogs.settings_gesture
            && activation_allowed
        {
            self.settings = gesture.staged.clone();
            self.accent = gesture.staged_accent;
        }
        let mut credentials_saved = false;
        let mut open_shortcuts = false;
        let mut open_calendar = false;
        let mut bounds = Dialog::bounds(ctx);
        if !self.any_modal_open()
            && let Some(strip) = self.toasts.bottom_strip(ctx)
        {
            bounds.max.y = bounds.max.y.min((strip.top() - 4.0).max(bounds.top()));
        }
        const CONTENT_WIDTH: f32 = 440.0;
        let style = ctx.global_style();
        let frame = egui::Frame::window(&style);
        // Leave room for the minimum-width body without requiring an initial horizontal scroll.
        let default_width =
            CONTENT_WIDTH + frame.total_margin().sum().x + style.spacing.scroll.allocated_width();
        let labels = i18n::dialog_labels();
        let response = Dialog::new(egui::Id::unique("thinoda_settings"), t("Settings"))
            .icon(icons::SETTINGS)
            .labels(&labels)
            .enabled(!self.dialogs.shortcuts)
            .min_size(egui::Vec2::ZERO)
            .default_width(default_width)
            .constrain_to(bounds)
            .hscroll(true)
            .show(ctx, |ui| {
                if !ui.is_enabled() {
                    activation_allowed = false;
                    if let Some(gesture) = &mut self.dialogs.settings_gesture {
                        gesture.canceled = true;
                    }
                }
                ui.set_min_width(CONTENT_WIDTH);
                thinoda_ui::tab_bar(
                    ui,
                    &mut self.dialogs.settings_tab,
                    &[
                        (SettingsTab::General, icons::TUNE, t("General")),
                        (SettingsTab::Editor, icons::EDIT, t("Editor")),
                        (SettingsTab::Ai, icons::AI, t("AI Assistant")),
                        (SettingsTab::Cloud, icons::CLOUD, t("Cloud")),
                    ],
                );
                if !activation_allowed {
                    self.dialogs.settings_tab = before_tab;
                }
                ui.horizontal_wrapped(|ui| {
                    open_shortcuts = ui
                        .add(egui::Button::new(t("Keyboard Shortcuts")))
                        .clicked()
                        && activation_allowed;
                    open_calendar = ui
                        .add_enabled(
                            self.doc().is_some(),
                            egui::Button::new(t("Work Calendar")),
                        )
                        .clicked()
                        && activation_allowed;
                });
                ui.separator();
                if self.dialogs.settings_tab == SettingsTab::General {
                    #[cfg(target_arch = "wasm32")]
                    if ui.button(t("Install web app")).clicked()
                        && activation_allowed
                        && let Err(error) = thinoda_platform::web::install_app(
                            t("Install web app"),
                            t("Close"),
                            t("Install from your browser menu. On iPhone/iPad use Share > Add to Home Screen; on Mac Safari use Share > Add to Dock. On Windows/Linux use Chrome or Edge."),
                            t("Installation keeps browser storage limits. Download backups; clearing site data can delete maps. Offline use needs one complete online load."),
                            t("No install prompt is available. The app may already be installed, or your browser requires its menu. Use HTTPS or localhost."),
                        )
                    {
                        log::error!("PWA install panel: {error}");
                        self.toasts.error(error);
                    }
                    widgets::section(ui, t("Appearance"), true, |ui| {
                        widgets::property_row(ui, t("Theme"), |ui| {
                            widgets::segmented(
                                ui,
                                &mut self.settings.theme,
                                &[
                                    (SettingsTheme::System, t("System")),
                                    (SettingsTheme::Light, t("Light")),
                                    (SettingsTheme::Dark, t("Dark")),
                                ],
                            );
                        });
                        widgets::property_row(ui, t("Accent Color"), |ui| {
                            if let Some(c) =
                                widgets::color_swatches(ui, &SWATCHES[..8], Some(self.accent))
                            {
                                self.accent = c;
                            }
                        });
                        widgets::property_row(ui, t("Language"), |ui| {
                            let mut l = self.settings.language;
                            egui::ComboBox::from_id_salt("settings_language")
                                .selected_text(if l == Language::Auto {
                                    t("Auto")
                                } else {
                                    l.native_name()
                                })
                                .show_ui(ui, |ui| {
                                    for language in Language::ALL {
                                        ui.selectable_value(
                                            &mut l,
                                            language,
                                            if language == Language::Auto {
                                                t("Auto")
                                            } else {
                                                language.native_name()
                                            },
                                        );
                                    }
                                })
                                .response
                                .accessible_name(t("Language"));
                            if l != self.settings.language {
                                self.settings.language = l;
                            }
                        });
                        widgets::property_field(ui, t("UI Scale"), |ui| {
                            ui.add(
                                egui::Slider::new(&mut self.settings.ui_scale, 0.75..=2.0)
                                    .step_by(0.05),
                            )
                        });
                    });
                    widgets::section(ui, t("Files"), true, |ui| {
                        widgets::property_field(ui, t("Autosave"), |ui| {
                            widgets::toggle(ui, &mut self.settings.autosave, t("Autosave"))
                        });
                        widgets::property_field(ui, t("Autosave interval (s)"), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.settings.autosave_interval_secs)
                                    .range(5..=3600),
                            )
                        });
                        widgets::property_field(ui, t("Keep backup on save"), |ui| {
                            widgets::toggle(
                                ui,
                                &mut self.settings.backup_on_save,
                                t("Keep backup on save"),
                            )
                        });
                        widgets::property_field(ui, t("Confirm before deleting"), |ui| {
                            widgets::toggle(
                                ui,
                                &mut self.settings.confirm_on_delete,
                                t("Confirm before deleting"),
                            )
                        });
                        if let Some(p) = &self.services.paths
                            && !crate::files::is_web()
                            && ui.button(t("Open settings folder")).clicked()
                            && activation_allowed
                            && let Err(error) =
                                thinoda_platform::system::reveal_in_file_manager(&p.config_dir)
                        {
                            log::error!("open settings folder: {error}");
                            self.toasts.error(error.to_string());
                        }
                    });
                }
                if self.dialogs.settings_tab == SettingsTab::Editor {
                    widgets::section(ui, t("Editor"), true, |ui| {
                        let control_width = (bounds.width()
                            - frame.total_margin().sum().x
                            - style.spacing.scroll.allocated_width()
                            - ui.spacing().indent)
                            .max(40.0)
                            .min(ui.available_width());
                        ui.push_id("alignment_guides", |ui| {
                            ui.set_width(control_width);
                            ui.horizontal_wrapped(|ui| {
                                widgets::toggle(
                                    ui,
                                    &mut self.settings.show_alignment_guides,
                                    t("Show alignment guides"),
                                );
                                ui.label(t("Show alignment guides"));
                            });
                            widgets::caption(
                                ui,
                                t("Visual guides while moving topics freely; positions do not snap."),
                            );
                        });
                        if !crate::files::is_web() {
                            ui.push_id("default_cjk_font", |ui| {
                                ui.set_width(control_width);
                                let label = ui.label(t("Default CJK font"));
                                let selected = self.settings.default_cjk_font.clone()
                                    .unwrap_or_else(|| t("Auto").to_owned());
                                font_choice(
                                    ui,
                                    &mut self.fonts,
                                    &mut self.settings.default_cjk_font,
                                    t("Auto"),
                                    false,
                                    None,
                                )
                                .labelled_by(label.id)
                                .on_hover_text(selected);
                            });
                            widgets::caption(
                                ui,
                                t("Fallback only; choose an installed font covering your script. Document fonts stay unchanged."),
                            );
                        }
                        widgets::property_field(ui, t("Compact Ribbon"), |ui| {
                            widgets::toggle(ui, &mut self.settings.compact_ribbon, t("Compact Ribbon"))
                        });
                        widgets::property_field(ui, t("Show topic toolbar"), |ui| {
                            widgets::toggle(
                                ui,
                                &mut self.settings.show_topic_toolbar,
                                t("Show topic toolbar"),
                            )
                        });
                        widgets::property_field(ui, t("Show topic add buttons"), |ui| {
                            widgets::toggle(
                                ui,
                                &mut self.settings.show_topic_add_buttons,
                                t("Show topic add buttons"),
                            )
                        });
                        widgets::property_field(ui, t("Initial canvas zoom"), |ui| {
                            ui.add(
                                egui::Slider::new(
                                    &mut self.settings.initial_canvas_zoom,
                                    0.1..=5.0,
                                )
                                .step_by(0.1)
                                .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
                            )
                        });
                        widgets::caption(
                            ui,
                            t("Applies to new and newly opened maps, not current views."),
                        );
                        widgets::property_field(
                            ui,
                            t("Reuse last structure for new blank maps"),
                            |ui| {
                                widgets::toggle(
                                    ui,
                                    &mut self.settings.reuse_last_structure,
                                    t("Reuse last structure for new blank maps"),
                                )
                            },
                        );
                        widgets::property_field(
                            ui,
                            t("Reuse last theme for new blank maps"),
                            |ui| {
                                widgets::toggle(
                                    ui,
                                    &mut self.settings.reuse_last_theme,
                                    t("Reuse last theme for new blank maps"),
                                )
                            },
                        );
                    });
                    self.menu_usage_section(ui, activation_allowed);
                }
                if self.dialogs.settings_tab == SettingsTab::Ai {
                    credentials_saved =
                        self.ai_settings_section(ui, activation_allowed && !pointer_down);
                }
                if self.dialogs.settings_tab == SettingsTab::Cloud {
                    self.cloud_settings_section(ui, activation_allowed && !pointer_down);
                    widgets::section(ui, t("Cloud"), true, |ui| {
                        ui.label(RichText::new(t("Detected cloud folders")).strong());
                        if self.services.cloud.is_empty() {
                            widgets::caption(ui, t("No cloud folders detected"));
                        }
                        for f in self.services.cloud.clone() {
                            ui.horizontal_wrapped(|ui| {
                                let sel =
                                    self.settings.cloud.preferred_folder.as_ref() == Some(&f.path);
                                ui.label(format!("{} {}", icons::CLOUD, f.name));
                                widgets::caption(ui, f.path.display().to_string());
                                if ui.add_enabled(!sel, egui::Button::new(t("Use"))).clicked() {
                                    self.settings.cloud.preferred_folder = Some(f.path.clone());
                                }
                            });
                        }
                        ui.label(t("Default save folder"));
                        ui.vertical(|ui| {
                            let label = self
                                .settings
                                .cloud
                                .preferred_folder
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "—".into());
                            widgets::caption(ui, label);
                            #[cfg(not(target_arch = "wasm32"))]
                            if ui.button(t("Choose…")).clicked()
                                && activation_allowed
                                && let Some(dir) = rfd::FileDialog::new().pick_folder()
                            {
                                self.settings.cloud.preferred_folder = Some(dir);
                            }
                        });
                    });
                }
                reveal_changed_focus(ui);
            });
        if response.close_requested {
            open = false;
        }
        if !activation_allowed {
            self.settings = before.clone();
            self.accent = before_accent;
            open = true;
        }
        if pointer_down && let Some(gesture) = &mut self.dialogs.settings_gesture {
            gesture.staged = self.settings.clone();
            gesture.staged_accent = self.accent;
            self.settings = before.clone();
            self.accent = before_accent;
        }
        if !pointer_down {
            self.dialogs.settings_gesture = None;
        }
        if self.settings.language != before.language && !pointer_down {
            i18n::set_lang(i18n::resolve(self.settings.language));
            let titles = crate::app::new_topic_titles();
            for d in &mut self.docs {
                d.view.titles = titles.clone();
            }
        }
        if self.settings != before && !credentials_saved && !pointer_down {
            self.save_settings();
        }
        self.dialogs.settings = open;
        if open_shortcuts {
            self.exec(ctx, Cmd::Shortcuts);
        }
        if open_calendar {
            self.dialogs.settings = false;
            self.exec(ctx, Cmd::WorkCalendar);
        }
    }
}

pub(crate) struct SettingsGesture {
    settings: thinoda_platform::settings::Settings,
    accent: Color32,
    staged: thinoda_platform::settings::Settings,
    staged_accent: Color32,
    frame: u64,
    canceled: bool,
}

/// Paints a colored dot (used for sheet tab status etc.).
pub fn dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, color);
}

/// A text field that edits a private buffer while focused and reports the new value once
/// focus is lost (so typing is not reset by the model value on every frame).
/// `rows == 0` renders a single-line field.
pub(crate) fn buffered_text(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
    rows: usize,
    hint: &str,
) -> (egui::Response, Option<String>) {
    let id = ui.make_persistent_id(("thinoda_buffered_text", salt));
    let mut buf: String = ui
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| current.to_string());
    let source_id = id.with("model_value");
    if ui
        .data(|data| data.get_temp::<String>(source_id))
        .is_some_and(|previous| previous != current && buf == previous)
    {
        buf = current.to_owned();
    }
    ui.data_mut(|data| data.insert_temp(source_id, current.to_owned()));
    let edit = if rows == 0 {
        TextEdit::singleline(&mut buf)
    } else {
        TextEdit::multiline(&mut buf).desired_rows(rows)
    };
    let r = ui.add(edit.id(id).hint_text(hint).desired_width(f32::INFINITY));
    if rows == 0 && r.lost_focus() {
        let cancel = ui.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::Enter);
            input.consume_key(Modifiers::NONE, Key::Escape)
        });
        if cancel {
            ui.data_mut(|data| data.remove::<String>(id));
            return (r, None);
        }
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf));
        (r, None)
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
        let done = (r.lost_focus() && buf != current).then_some(buf);
        (r, done)
    }
}

fn restore_inspector_buffer(
    doc: &crate::doc::OpenDoc,
    ui: &Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    topic: TopicId,
    field: &crate::doc::InspectorField,
) {
    if let Some(edit) = &doc.inspector_edit
        && edit.sheet_index == doc.doc.active_index()
        && edit.sheet_root == doc.doc.sheet().root
        && edit.topic == topic
        && &edit.field == field
    {
        let id = ui.make_persistent_id(("thinoda_buffered_text", salt));
        ui.data_mut(|data| {
            if data.get_temp::<String>(id).is_none() {
                data.insert_temp(id, edit.text.clone());
            }
        });
    }
}

fn track_inspector_edit(
    doc: &mut crate::doc::OpenDoc,
    ui: &Ui,
    response: &egui::Response,
    field: crate::doc::InspectorField,
    original: Option<thinoda_core::notes::NoteContent>,
    done: Option<String>,
    format_notes: bool,
) -> Result<(), String> {
    let sheet_root = doc.doc.sheet().root;
    let sheet_index = doc.doc.active_index();
    let topic = doc
        .view
        .primary()
        .expect("inspector requires a selected topic");
    let same_source = doc.inspector_edit.as_ref().is_some_and(|edit| {
        edit.field == field
            && edit.sheet_index == sheet_index
            && edit.sheet_root == sheet_root
            && edit.topic == topic
    });
    let text = if response.has_focus() {
        ui.data(|data| data.get_temp::<String>(response.id))
    } else {
        done.clone()
    };
    if let Some(text) = text {
        let (original, format_notes) = if same_source {
            let previous = doc.inspector_edit.as_ref().unwrap();
            (
                previous.original.clone(),
                previous.format_notes || format_notes,
            )
        } else {
            doc.flush_inspector_edit(ui.ctx())?;
            (original, format_notes)
        };
        doc.inspector_edit = (text != original.as_deref().unwrap_or_default() || format_notes)
            .then_some(crate::doc::InspectorEdit {
                buffer_id: Some(response.id),
                sheet_index,
                sheet_root,
                topic,
                field,
                original,
                format_notes,
                text,
            });
        if done.is_some() {
            doc.flush_inspector_edit(ui.ctx())?;
        }
    } else if response.lost_focus() && same_source {
        doc.inspector_edit = None;
    }
    Ok(())
}

#[cfg(test)]
#[path = "card_text_tests.rs"]
mod card_text_tests;
