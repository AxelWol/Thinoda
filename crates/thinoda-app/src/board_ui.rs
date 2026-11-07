//! Board tools and accessible labels shared by native and web views.

use egui::{Color32, Context, Rect, Ui, vec2};
use thinoda_canvas::{BoardTool, MapView};
use thinoda_core::{
    CanvasObject, CanvasObjectId, CanvasObjectKind, Color, ConnectorMarker, ConnectorMarkerKind,
    ConnectorRoute, InkTool, ObjectStyle, ShapeKind, TopicId,
    board::{
        BoardMember, DEFAULT_CONNECTOR_MARKER_SIZE, MAX_BOARD_COORDINATE,
        MAX_CONNECTOR_MARKER_SIZE, MIN_CARD_SIZE,
    },
};
use thinoda_ui::{
    icons,
    ribbon::{self, CommandIcon, Detail},
    widgets,
};

use crate::{
    app::{CenterMode, Cmd, ThinodaApp},
    command_icons::board_tool_glyph,
    i18n::{self, t},
};

/// Hint shown while a shape tool is armed (draw.io-style placement).
pub(crate) const ARMED_SHAPE_HINT: &str = "Click empty space to add · Esc to finish";
/// Narrower canvases skip the hint so it never covers the minimap or toolbars.
pub(crate) const ARMED_SHAPE_HINT_MIN_WIDTH: f32 = 520.0;

fn armed_shape_hint_dismissed_id() -> egui::Id {
    egui::Id::unique("thinoda_armed_shape_hint_dismissed")
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ConnectorOwner {
    viewport: egui::ViewportId,
    document: String,
    instance: u64,
    sheet: usize,
    root: TopicId,
    object: CanvasObjectId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ConnectorTarget {
    owner: ConnectorOwner,
    revision: u64,
}

#[derive(Clone, Copy)]
struct ConnectorProperties {
    route: ConnectorRoute,
    arrow_start: bool,
    arrow_end: bool,
    color: Option<Color>,
    effective_color: Color,
    width: f32,
    has_waypoints: bool,
}

struct ConnectorContext {
    target: ConnectorTarget,
    enabled: bool,
    properties: ConnectorProperties,
}

#[derive(Clone)]
struct ConnectorPopups {
    target: ConnectorTarget,
    ids: Vec<egui::Id>,
}

#[derive(Clone, Copy)]
struct ShapeProperties {
    shape: ShapeKind,
    fill: Option<Color>,
    stroke: Option<Color>,
    /// Outline drawn when no explicit stroke is set and the shape has no fill.
    auto_stroke: Color,
    stroke_width: f32,
}

impl ShapeProperties {
    fn visible_stroke(self) -> Option<Color> {
        self.stroke
            .or_else(|| self.fill.is_none().then_some(self.auto_stroke))
    }
}

struct ShapeContext {
    target: ConnectorTarget,
    enabled: bool,
    properties: ShapeProperties,
}

enum ShapeChange {
    Shape(ShapeKind),
    Fill(Option<Color>),
    Stroke(Option<Color>),
    StrokeWidth(f32),
}

impl ShapeChange {
    fn label(&self) -> &'static str {
        match self {
            Self::Shape(_) => "Change shape",
            Self::Fill(_) => "Fill color",
            Self::Stroke(_) => "Outline color",
            Self::StrokeWidth(_) => "Outline width",
        }
    }

    fn apply(self, object: &mut CanvasObject, auto_stroke: Color) {
        match self {
            Self::Shape(kind) => {
                if let CanvasObjectKind::Shape { shape, .. } = &mut object.kind {
                    *shape = kind;
                }
            }
            Self::Fill(color) => object.style.fill = color,
            Self::Stroke(color) => object.style.stroke = color,
            Self::StrokeWidth(width) => {
                object.style.stroke_width = width;
                // An automatic outline has a fixed width, so a chosen width pins its color.
                object.style.stroke.get_or_insert(auto_stroke);
            }
        }
    }
}

/// Shortcut shown below the contextual board-object controls.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextAction {
    EditLabel,
    FormatPanel,
}

#[derive(Clone, Copy)]
struct ColorControl {
    label: &'static str,
    none: &'static str,
    key: &'static str,
    icon: &'static str,
}

#[derive(Clone, Copy)]
struct WidthControl {
    label: &'static str,
    key: &'static str,
    salt: &'static str,
}

const CONNECTOR_COLOR: ColorControl = ColorControl {
    label: "Connector color",
    none: "Auto",
    key: "connector",
    icon: icons::FORMAT,
};
const CONNECTOR_WIDTH: WidthControl = WidthControl {
    label: "Connector width",
    key: "connector",
    salt: "connector-width",
};
const SHAPE_FILL: ColorControl = ColorControl {
    label: "Fill color",
    none: "No fill",
    key: "shape_fill",
    icon: icons::FORMAT,
};
const SHAPE_OUTLINE: ColorControl = ColorControl {
    label: "Outline color",
    none: "Auto",
    key: "shape_outline",
    icon: icons::TOPIC_STYLE,
};
const SHAPE_OUTLINE_WIDTH: WidthControl = WidthControl {
    label: "Outline width",
    key: "shape_outline",
    salt: "shape-outline-width",
};

#[derive(Clone, Copy)]
struct ColorDraft {
    owner: egui::Id,
    value: egui::ecolor::Hsva,
    frame: u64,
    canceled: bool,
}

#[derive(Clone, Copy)]
struct WidthDraft {
    owner: egui::Id,
    value: f32,
    frame: u64,
}

enum ConnectorChange {
    Route(ConnectorRoute),
    ArrowStart(bool),
    ArrowEnd(bool),
    Color(Option<Color>),
    Width(f32),
    ClearWaypoints,
}

impl ConnectorChange {
    fn label(&self) -> &'static str {
        match self {
            Self::Route(_) => "Connector route",
            Self::ArrowStart(_) => "Arrow at start",
            Self::ArrowEnd(_) => "Arrow at end",
            Self::Color(_) => "Connector color",
            Self::Width(_) => "Connector width",
            Self::ClearWaypoints => "Clear connector waypoints",
        }
    }

    fn apply(self, connector: &mut thinoda_core::CanvasConnector) {
        match self {
            Self::Route(route) => connector.route = route,
            Self::ArrowStart(enabled) => connector.set_arrow_start(enabled),
            Self::ArrowEnd(enabled) => connector.set_arrow_end(enabled),
            Self::Color(color) => connector.color = color,
            Self::Width(width) => connector.width = width,
            Self::ClearWaypoints => connector.waypoints = Default::default(),
        }
    }
}

fn connector_popup_slot(viewport: egui::ViewportId) -> egui::Id {
    egui::Id::unique(("board_connector_popups", viewport))
}

#[derive(Clone, Copy)]
pub(crate) enum BoardStyleClipboard {
    Shape(ObjectStyle),
    Connector {
        style: ObjectStyle,
        color: Option<Color>,
        width: f32,
        route: ConnectorRoute,
        arrow_start: bool,
        arrow_end: bool,
        start_marker: Option<ConnectorMarker>,
        end_marker: Option<ConnectorMarker>,
        start_marker_size: Option<f32>,
        end_marker_size: Option<f32>,
        start_marker_unfilled: bool,
        end_marker_unfilled: bool,
    },
    Ink {
        style: ObjectStyle,
        color: Color,
        width: f32,
        tool: InkTool,
    },
}

impl BoardStyleClipboard {
    pub(crate) fn from_object(object: &CanvasObject) -> Self {
        match &object.kind {
            CanvasObjectKind::Shape { .. } => Self::Shape(object.style),
            CanvasObjectKind::Connector(connector) => Self::Connector {
                style: object.style,
                color: connector.color,
                width: connector.width,
                route: connector.route,
                arrow_start: connector.arrow_start,
                arrow_end: connector.arrow_end,
                start_marker: connector.start_marker,
                end_marker: connector.end_marker,
                start_marker_size: connector.start_marker_size,
                end_marker_size: connector.end_marker_size,
                start_marker_unfilled: connector.start_marker_unfilled,
                end_marker_unfilled: connector.end_marker_unfilled,
            },
            CanvasObjectKind::Ink(ink) => Self::Ink {
                style: object.style,
                color: ink.color,
                width: ink.width,
                tool: ink.tool,
            },
        }
    }

    pub(crate) fn matches(self, object: &CanvasObject) -> bool {
        matches!(
            (self, &object.kind),
            (Self::Shape(_), CanvasObjectKind::Shape { .. })
                | (Self::Connector { .. }, CanvasObjectKind::Connector(_))
                | (Self::Ink { .. }, CanvasObjectKind::Ink(_))
        )
    }

    pub(crate) fn apply(self, object: &mut CanvasObject) {
        match (self, &mut object.kind) {
            (Self::Shape(style), CanvasObjectKind::Shape { .. }) => object.style = style,
            (
                Self::Connector {
                    style,
                    color,
                    width,
                    route,
                    arrow_start,
                    arrow_end,
                    start_marker,
                    end_marker,
                    start_marker_size,
                    end_marker_size,
                    start_marker_unfilled,
                    end_marker_unfilled,
                },
                CanvasObjectKind::Connector(connector),
            ) => {
                object.style = style;
                connector.color = color;
                connector.width = width;
                connector.route = route;
                connector.arrow_start = arrow_start;
                connector.arrow_end = arrow_end;
                connector.start_marker = start_marker;
                connector.end_marker = end_marker;
                connector.start_marker_size = start_marker_size;
                connector.end_marker_size = end_marker_size;
                connector.start_marker_unfilled = start_marker_unfilled;
                connector.end_marker_unfilled = end_marker_unfilled;
            }
            (
                Self::Ink {
                    style,
                    color,
                    width,
                    tool,
                },
                CanvasObjectKind::Ink(ink),
            ) => {
                object.style = style;
                ink.color = color;
                ink.width = width;
                ink.tool = tool;
            }
            _ => unreachable!("style compatibility is checked before editing"),
        }
    }
}

pub(crate) fn shape_name(shape: ShapeKind) -> &'static str {
    match shape {
        ShapeKind::Rectangle => "Rectangle",
        ShapeKind::RoundedRectangle => "Rounded rectangle",
        ShapeKind::Ellipse => "Ellipse",
        ShapeKind::Diamond => "Diamond",
        ShapeKind::Triangle => "Triangle",
        ShapeKind::RightTriangle => "Right triangle",
        ShapeKind::Parallelogram => "Parallelogram",
        ShapeKind::Trapezoid => "Trapezoid",
        ShapeKind::Pentagon => "Pentagon",
        ShapeKind::Hexagon => "Hexagon",
        ShapeKind::Octagon => "Octagon",
        ShapeKind::Star => "Star",
        ShapeKind::Plus => "Plus",
        ShapeKind::Arrow => "Arrow",
        ShapeKind::DoubleArrow => "Double arrow",
        ShapeKind::Chevron => "Chevron",
        ShapeKind::Cloud => "Cloud",
        ShapeKind::Callout => "Callout",
        ShapeKind::Cylinder => "Cylinder",
        ShapeKind::Document => "Document",
        ShapeKind::Process => "Process",
        ShapeKind::PredefinedProcess => "Predefined process",
        ShapeKind::Decision => "Decision",
        ShapeKind::Terminator => "Terminator",
        ShapeKind::Data => "Data",
        ShapeKind::ManualInput => "Manual input",
        ShapeKind::Preparation => "Preparation",
        ShapeKind::Delay => "Delay",
        ShapeKind::OffPageConnector => "Off-page connector",
    }
}

pub(crate) fn configure_board_labels(ctx: &Context, key: &str, view: &mut MapView) {
    let id = egui::Id::unique(("board_labels_language", key));
    let language = i18n::lang();
    if ctx.data(|data| data.get_temp::<i18n::Lang>(id)) == Some(language) {
        return;
    }
    let labels = &mut view.board_labels;
    labels.sticky = t("Sticky note").into();
    labels.text = t("Text card").into();
    labels.image = t("Image card").into();
    labels.shape = t("Shape").into();
    labels.shape_names = ShapeKind::ALL
        .into_iter()
        .map(|shape| (shape, t(shape_name(shape)).into()))
        .collect();
    labels.connector = t("Connector").into();
    labels.ink = t("Ink stroke").into();
    labels.frame = t("Frame").into();
    labels.new_frame = t("Frame").into();
    labels.selected = t("Selected").into();
    labels.locked = t("Locked").into();
    labels.from = t("From").into();
    labels.to = t("To").into();
    labels.free_point = t("Free point").into();
    labels.resize = t("Resize board item").into();
    labels.rotate = t("Rotate board item").into();
    labels.waypoint = t("Connector waypoint").into();
    labels.insert_waypoint = t("Add connector waypoint").into();
    labels.waypoint_hint =
        t("Drag or use arrow keys to move; double-click or press Delete to remove.").into();
    labels.insert_waypoint_hint =
        t("Drag, press Enter, or use arrow keys to add a waypoint.").into();
    labels.connect_up = t("Add connected shape above").into();
    labels.connect_right = t("Add connected shape to the right").into();
    labels.connect_down = t("Add connected shape below").into();
    labels.connect_left = t("Add connected shape to the left").into();
    labels.connect_hint = t("Click adds a connected copy; Ctrl+click adds only a copy; Shift+click adds only a connector. Drag to connect to another item; Ctrl+drag drops a connected copy. Use Command on macOS.").into();
    labels.quick_hint =
        t("Hold an arrow, or press Shift+F10 on a focused arrow, to choose the shape.").into();
    labels.label_editor = t("Board object label").into();
    view.minimap_labels.accessible_name = t(
        "Board minimap. Click or drag to pan; arrow keys move the view; Shift makes larger steps.",
    )
    .into();
    view.minimap_labels.tooltip =
        t("Click or drag to pan. Arrow keys move the view; Shift makes larger steps.").into();
    view.minimap_labels.viewport_offscreen = t("The current view is outside the minimap.").into();
    ctx.data_mut(|data| data.insert_temp(id, language));
}

impl ThinodaApp {
    /// The one selected board object a contextual toolbar may edit, and whether editing is allowed.
    fn single_board_object(&self) -> Option<(ConnectorTarget, bool)> {
        let doc = self.doc()?;
        let view = &doc.view;
        if !matches!(view.board_tool, BoardTool::Select | BoardTool::Connector)
            || view.board_selection.len() != 1
            || !view.selection.is_empty()
            || view.selected_object.is_some()
            || view.relationship_from.is_some()
            || view.editing_topic().is_some()
            || view.editing_board_object().is_some()
        {
            return None;
        }
        let sheet = doc.doc.sheet();
        let id = view.board_selection[0];
        let object = sheet.canvas.object(id)?;
        let enabled = !object.locked
            && object.group.is_none()
            && !view.has_active_canvas_gesture()
            && !doc.has_inspector_changes()
            && doc.fim_edit.is_none()
            && doc.ensure_no_pending_ink().is_ok()
            && self.mode == CenterMode::Map
            && !self.task_table_visible()
            && !self.zen
            && !self.search.open
            && !self.palette.open
            && !self.any_modal_open()
            && !self.dialogs.settings
            && !self.dialogs.about;
        Some((
            ConnectorTarget {
                owner: ConnectorOwner {
                    viewport: self.current_viewport,
                    document: doc.key.clone(),
                    instance: doc.doc.instance_id(),
                    sheet: doc.doc.active_index(),
                    root: sheet.root,
                    object: id,
                },
                revision: doc.doc.state_id(),
            },
            enabled,
        ))
    }

    fn connector_context(&self) -> Option<ConnectorContext> {
        let (target, enabled) = self.single_board_object()?;
        let doc = self.doc()?;
        let view = &doc.view;
        let object = doc.doc.sheet().canvas.object(target.owner.object)?;
        let CanvasObjectKind::Connector(connector) = &object.kind else {
            return None;
        };
        let default_color = thinoda_canvas::automatic_board_stroke(view.accent, view.dark);
        let effective_color = connector
            .color
            .or(object.style.stroke)
            .unwrap_or_else(|| crate::ui::to_color(default_color));
        Some(ConnectorContext {
            target,
            enabled,
            properties: ConnectorProperties {
                route: connector.route,
                arrow_start: connector.arrow_start,
                arrow_end: connector.arrow_end,
                color: connector.color,
                effective_color,
                width: connector.width,
                has_waypoints: !connector.waypoints.is_empty(),
            },
        })
    }

    fn shape_context(&self) -> Option<ShapeContext> {
        let (target, enabled) = self.single_board_object()?;
        let doc = self.doc()?;
        let view = &doc.view;
        let object = doc.doc.sheet().canvas.object(target.owner.object)?;
        let CanvasObjectKind::Shape { shape, .. } = &object.kind else {
            return None;
        };
        Some(ShapeContext {
            target,
            enabled,
            properties: ShapeProperties {
                shape: *shape,
                fill: object.style.fill,
                stroke: object.style.stroke,
                auto_stroke: crate::ui::to_color(thinoda_canvas::automatic_board_stroke(
                    view.accent,
                    view.dark,
                )),
                stroke_width: object.style.stroke_width,
            },
        })
    }

    fn apply_shape_change(
        &mut self,
        target: &ConnectorTarget,
        change: ShapeChange,
    ) -> Result<ConnectorTarget, String> {
        let Some(context) = self
            .shape_context()
            .filter(|current| current.enabled && current.target == *target)
        else {
            return Err(t("Not possible here").into());
        };
        let label = change.label();
        let auto_stroke = context.properties.auto_stroke;
        let doc = self
            .doc_mut()
            .ok_or_else(|| t("Not possible here").to_owned())?;
        doc.doc
            .update_canvas_object(label, target.owner.object, |object| {
                change.apply(object, auto_stroke);
            })
            .map_err(|error| error.to_string())?;
        Ok(ConnectorTarget {
            revision: doc.doc.state_id(),
            ..target.clone()
        })
    }

    /// Starts inline editing of the selected shape or connector label, as F2 does.
    fn edit_board_label(&mut self, target: &ConnectorTarget) -> Result<(), String> {
        if !self
            .single_board_object()
            .is_some_and(|(current, enabled)| enabled && current == *target)
        {
            return Err(t("Not possible here").into());
        }
        let doc = self
            .doc_mut()
            .ok_or_else(|| t("Not possible here").to_owned())?;
        if doc.view.begin_board_label(&doc.doc, target.owner.object) {
            Ok(())
        } else {
            Err(t("Not possible here").into())
        }
    }

    fn apply_connector_change(
        &mut self,
        target: &ConnectorTarget,
        change: ConnectorChange,
    ) -> Result<ConnectorTarget, String> {
        if !self
            .connector_context()
            .is_some_and(|current| current.enabled && current.target == *target)
        {
            return Err(t("Not possible here").into());
        }
        let label = change.label();
        let doc = self
            .doc_mut()
            .ok_or_else(|| t("Not possible here").to_owned())?;
        doc.doc
            .update_canvas_object(label, target.owner.object, |object| {
                let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                    unreachable!("connector ownership was checked before the edit")
                };
                change.apply(connector);
            })
            .map_err(|error| error.to_string())?;
        Ok(ConnectorTarget {
            revision: doc.doc.state_id(),
            ..target.clone()
        })
    }

    pub(crate) fn close_connector_popups(ctx: &Context, viewport: egui::ViewportId) {
        let slot = connector_popup_slot(viewport);
        let popups = ctx.data_mut(|data| {
            let popups = data.get_temp::<ConnectorPopups>(slot);
            data.remove::<ConnectorPopups>(slot);
            popups
        });
        if let Some(popups) = popups {
            for id in popups.ids {
                egui::Popup::close_id(ctx, id);
            }
        }
    }

    pub(crate) fn board_toolbar(
        &mut self,
        ctx: &Context,
        canvas: Rect,
        editing_toolbar: Option<Rect>,
    ) -> Option<Rect> {
        let connector = self.connector_context();
        let shape = connector.is_none().then(|| self.shape_context()).flatten();
        let context_target = connector
            .as_ref()
            .map(|value| (&value.target, value.enabled))
            .or_else(|| shape.as_ref().map(|value| (&value.target, value.enabled)));
        let viewport = self.current_viewport;
        let popup_slot = connector_popup_slot(viewport);
        let previous_popups = ctx.data(|data| data.get_temp::<ConnectorPopups>(popup_slot));
        if previous_popups.as_ref().is_some_and(|previous| {
            !context_target.is_some_and(|(target, enabled)| enabled && *target == previous.target)
        }) {
            Self::close_connector_popups(ctx, viewport);
        }
        let format_panel_open = self.settings.show_format_panel
            && self.sidebar_family == crate::app::SidebarFamily::Format;
        let doc = self.doc()?;
        let tool = doc.view.board_tool;
        let show_minimap = doc.view.show_minimap;
        let mut ink_settings = doc.view.ink_settings;
        let collapsed_id = egui::Id::unique(("board_toolbar_collapsed", &doc.key));
        let auto_color_id = egui::Id::unique(("board_ink_auto_color", &doc.key));
        let last_tool_id = egui::Id::unique(("board_toolbar_last_tool", &doc.key));
        let last_connector_id = egui::Id::unique(("board_toolbar_last_connector", viewport));
        let connector_owner = context_target.map(|(target, _)| target.owner.clone());
        let reveal_connector = connector_owner.is_some()
            && ctx
                .data(|data| data.get_temp::<Option<ConnectorOwner>>(last_connector_id))
                .flatten()
                != connector_owner;
        let reveal_context = reveal_connector
            || matches!(
                tool,
                BoardTool::Pen
                    | BoardTool::Highlighter
                    | BoardTool::ObjectEraser
                    | BoardTool::PixelEraser
            ) && ctx.data(|data| data.get_temp::<BoardTool>(last_tool_id)) != Some(tool);
        let mut auto_color = ctx
            .data(|data| data.get_temp::<bool>(auto_color_id))
            .unwrap_or(ink_settings.color == Color::BLACK);
        let background = thinoda_core::theme::Theme::for_settings(&doc.doc.sheet().theme)
            .background(&doc.doc.sheet().theme, doc.view.dark);
        let default_ink_color = Color::highest_contrast_on(background);
        if auto_color {
            ink_settings.color = default_ink_color;
        }
        let mut collapsed = ctx
            .data(|data| data.get_temp::<bool>(collapsed_id))
            .unwrap_or(true);
        let mut command = None;
        let mut connector_change = None;
        let mut shape_change = None;
        let mut context_action = None;
        let mut insert_shape = None;
        let mut connector_popups = Vec::new();
        let content_width = (canvas.width() - 36.0).max(0.0);
        let toolbar_width = if collapsed {
            content_width.min(34.0)
        } else {
            content_width.min(200.0)
        };
        let toolbar_left = canvas.left() + 12.0;
        let toolbar_right = toolbar_left + toolbar_width + 16.0;
        let editing_toolbar = editing_toolbar
            .filter(|rect| rect.right() > toolbar_left && rect.left() < toolbar_right);
        let top_inset = editing_toolbar
            .map_or(8.0, |rect| (rect.bottom() - canvas.top() + 8.0).max(8.0))
            .min((canvas.height() - 100.0).max(0.0));
        let row_height = ((canvas.height() - 20.0) * 0.5).clamp(20.0, 34.0);
        let inner_height = (canvas.height() - top_inset - 16.0).max(0.0);
        let toolbar = egui::Area::new(egui::Id::unique("thinoda_board_toolbar"))
            .order(egui::Order::Middle)
            .fixed_pos(canvas.left_top() + vec2(12.0, top_inset))
            .constrain_to(canvas)
            .movable(false)
            .show(ctx, |ui| {
                widgets::card(ui).show(ui, |ui| {
                    ui.set_width(toolbar_width);
                    ui.set_max_height(inner_height);
                    let inner_top = ui.cursor().min.y;
                    let (toggle_icon, toggle_label) = if collapsed {
                        (icons::ARROW_RIGHT, "Expand board toolbar")
                    } else {
                        (icons::ARROW_LEFT, "Collapse board toolbar")
                    };
                    if board_tool_button(
                        ui,
                        toggle_icon.into(),
                        toggle_label,
                        false,
                        collapsed,
                        row_height,
                    )
                    .1
                    {
                        collapsed = !collapsed;
                    }
                    let available_height =
                        (inner_height - (ui.cursor().min.y - inner_top)).max(0.0);
                    let mut scroll = egui::ScrollArea::vertical()
                        .id_salt("board_toolbar_scroll")
                        .min_scrolled_height(0.0)
                        .max_height(available_height);
                    if reveal_context {
                        scroll = scroll.vertical_scroll_offset(0.0);
                    }
                    scroll.show(ui, |ui| {
                        if let Some(connector) = &connector {
                            ui.push_id(&connector.target.owner, |ui| {
                                ui.add_enabled_ui(connector.enabled, |ui| {
                                    let (change, popups) = connector_context_controls(
                                        ui, canvas, collapsed, row_height, connector,
                                    );
                                    connector_change = change;
                                    connector_popups = popups;
                                });
                                context_action = context_action_controls(
                                    ui,
                                    collapsed,
                                    row_height,
                                    connector.enabled,
                                    format_panel_open,
                                );
                            });
                            ui.separator();
                        } else if let Some(shape) = &shape {
                            ui.push_id(&shape.target.owner, |ui| {
                                ui.add_enabled_ui(shape.enabled, |ui| {
                                    let (change, popups) = shape_context_controls(
                                        ui, canvas, collapsed, row_height, shape,
                                    );
                                    shape_change = change;
                                    connector_popups = popups;
                                });
                                context_action = context_action_controls(
                                    ui,
                                    collapsed,
                                    row_height,
                                    shape.enabled,
                                    format_panel_open,
                                );
                            });
                            ui.separator();
                        } else if matches!(tool, BoardTool::Pen | BoardTool::Highlighter) {
                            let color_control = board_tool_popup(
                                ui,
                                icons::FORMAT.into(),
                                "Ink color",
                                collapsed,
                                false,
                                row_height,
                                |ui| {
                                    ui.set_width((canvas.width() - 24.0).clamp(1.0, 164.0));
                                    reserve_popup_scrollbar_space(ui);
                                    let max_height = (canvas.height() - 32.0).clamp(1.0, 180.0);
                                    egui::ScrollArea::vertical()
                                        .id_salt("board_ink_color_choices")
                                        .min_scrolled_height(max_height.min(120.0))
                                        .max_height(max_height)
                                        .show(ui, |ui| {
                                            ui.checkbox(&mut auto_color, t("Auto"));
                                            let selected = (!auto_color).then(|| {
                                                Color32::from_rgb(
                                                    ink_settings.color.r,
                                                    ink_settings.color.g,
                                                    ink_settings.color.b,
                                                )
                                            });
                                            if let Some(color) = widgets::color_swatches(
                                                ui,
                                                &crate::ui::SWATCHES,
                                                selected,
                                            ) {
                                                let [r, g, b, _] = color.to_srgba_unmultiplied();
                                                ink_settings.color =
                                                    Color::rgba(r, g, b, ink_settings.color.a);
                                                auto_color = false;
                                            }
                                            if !auto_color {
                                                let mut color =
                                                    crate::ui::from_color(ink_settings.color);
                                                if ui
                                                    .color_edit_button_srgba(&mut color)
                                                    .accessible_name(t("Ink color"))
                                                    .changed()
                                                {
                                                    let [r, g, b, a] =
                                                        color.to_srgba_unmultiplied();
                                                    ink_settings.color = Color::rgba(r, g, b, a);
                                                }
                                            }
                                        });
                                },
                            );
                            let color = if auto_color {
                                default_ink_color
                            } else {
                                ink_settings.color
                            };
                            let swatch = Rect::from_min_size(
                                color_control.rect.left_bottom() + vec2(8.0, -6.0),
                                vec2(16.0, 3.0),
                            );
                            ui.painter().rect_filled(
                                swatch,
                                1,
                                Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a),
                            );
                            ui.painter().rect_stroke(
                                swatch,
                                1,
                                egui::Stroke::new(1.0, thinoda_ui::Palette::of_ui(ui).border),
                                egui::StrokeKind::Inside,
                            );
                            board_tool_popup(
                                ui,
                                icons::STROKE_WIDTH.into(),
                                "Ink width",
                                collapsed,
                                false,
                                row_height,
                                |ui| {
                                    ui.set_width((canvas.width() - 24.0).clamp(1.0, 206.0));
                                    reserve_popup_scrollbar_space(ui);
                                    let max_height = (canvas.height() - 32.0).clamp(1.0, 180.0);
                                    egui::ScrollArea::vertical()
                                        .id_salt("board_ink_width_choices")
                                        .min_scrolled_height(max_height.min(120.0))
                                        .max_height(max_height)
                                        .show(ui, |ui| {
                                            width_presets(ui, &mut ink_settings.width, "Ink width");
                                            ui.add(
                                                egui::DragValue::new(&mut ink_settings.width)
                                                    .range(
                                                    thinoda_core::board::MIN_STROKE_WIDTH
                                                        ..=thinoda_core::board::MAX_STROKE_WIDTH,
                                                ),
                                            )
                                            .accessible_name(t("Ink width"));
                                        });
                                },
                            );
                            board_tool_popup(
                                ui,
                                board_tool_glyph(tool),
                                "Stroke type",
                                collapsed,
                                false,
                                row_height,
                                |ui| {
                                    for (value, label) in [
                                        (BoardTool::Pen, "Pen"),
                                        (BoardTool::Highlighter, "Highlighter"),
                                    ] {
                                        if board_tool_button(
                                            ui,
                                            board_tool_glyph(value),
                                            label,
                                            value == tool,
                                            false,
                                            row_height,
                                        )
                                        .1
                                        {
                                            command = Some(Cmd::BoardTool(value));
                                            ui.close_kind(egui::UiKind::Menu);
                                        }
                                    }
                                },
                            );
                            board_tool_popup(
                                ui,
                                icons::OPACITY.into(),
                                "Opacity",
                                collapsed,
                                false,
                                row_height,
                                |ui| {
                                    ui.add(
                                        egui::DragValue::new(&mut ink_settings.opacity)
                                            .range(0.0..=1.0)
                                            .speed(0.01),
                                    )
                                    .accessible_name(t("Opacity"));
                                },
                            );
                            if auto_color {
                                ink_settings.color = default_ink_color;
                            }
                            ui.separator();
                        } else if matches!(tool, BoardTool::ObjectEraser | BoardTool::PixelEraser) {
                            board_tool_popup(
                                ui,
                                icons::STROKE_WIDTH.into(),
                                "Eraser radius",
                                collapsed,
                                false,
                                row_height,
                                |ui| {
                                    ui.add(
                                        egui::DragValue::new(&mut ink_settings.eraser_radius)
                                            .range(0.1..=200.0),
                                    )
                                    .accessible_name(t("Eraser radius"));
                                },
                            );
                            ui.separator();
                        }
                        for (value, label) in [
                            (BoardTool::Select, "Select tool"),
                            (BoardTool::Hand, "Hand tool"),
                            (BoardTool::Sticky, "Sticky note"),
                            (BoardTool::Text, "Text card"),
                        ] {
                            if board_tool_button(
                                ui,
                                board_tool_glyph(value),
                                label,
                                value == tool,
                                collapsed,
                                row_height,
                            )
                            .1
                            {
                                command = Some(Cmd::BoardTool(value));
                            }
                        }
                        board_tool_popup(
                            ui,
                            icons::BOARD_SHAPES.into(),
                            "Shape library",
                            collapsed,
                            matches!(tool, BoardTool::Shape(_)),
                            row_height,
                            |ui| {
                                let current = match tool {
                                    BoardTool::Shape(shape) => Some(shape),
                                    _ => None,
                                };
                                match shape_grid(ui, canvas, current, t, true) {
                                    Some(ShapePick::Choose(shape)) => {
                                        // Choosing the armed tile again finishes placing shapes.
                                        command = Some(Cmd::BoardTool(
                                            if tool == BoardTool::Shape(shape) {
                                                BoardTool::Select
                                            } else {
                                                BoardTool::Shape(shape)
                                            },
                                        ));
                                    }
                                    Some(ShapePick::Insert(shape)) => insert_shape = Some(shape),
                                    // The board takes the drop, so get the menu out of the way.
                                    Some(ShapePick::Dragged) => {}
                                    None => return,
                                }
                                ui.close_kind(egui::UiKind::Menu);
                            },
                        );
                        for (value, label) in [
                            (BoardTool::Connector, "Connector tool"),
                            (BoardTool::Frame, "Frame tool"),
                            (BoardTool::Pen, "Pen"),
                            (BoardTool::Highlighter, "Highlighter"),
                            (BoardTool::ObjectEraser, "Object eraser"),
                            (BoardTool::PixelEraser, "Pixel eraser"),
                            (BoardTool::Lasso, "Lasso"),
                        ] {
                            if board_tool_button(
                                ui,
                                board_tool_glyph(value),
                                label,
                                value == tool,
                                collapsed,
                                row_height,
                            )
                            .1
                            {
                                command = Some(Cmd::BoardTool(value));
                            }
                        }
                        if board_tool_button(
                            ui,
                            icons::IMAGE.into(),
                            "Image card",
                            false,
                            collapsed,
                            row_height,
                        )
                        .1
                        {
                            command = Some(Cmd::BoardImage);
                        }
                        ui.separator();
                        if board_tool_button(
                            ui,
                            icons::BOARD_MINIMAP.into(),
                            "Board minimap",
                            show_minimap,
                            collapsed,
                            row_height,
                        )
                        .1
                        {
                            command = Some(Cmd::BoardMinimap);
                        }
                        crate::ui::reveal_changed_focus(ui);
                    });
                });
            });
        ctx.data_mut(|data| {
            data.insert_temp(collapsed_id, collapsed);
            data.insert_temp(auto_color_id, auto_color);
            data.insert_temp(last_tool_id, tool);
            data.insert_temp(last_connector_id, connector_owner);
        });
        if let Some(doc) = self.doc_mut() {
            doc.view.ink_settings = ink_settings;
        }
        let action_target = context_target.map(|(target, _)| target.clone());
        if let Some(connector) = connector.filter(|value| value.enabled) {
            let mut target = connector.target;
            if let Some(change) = connector_change {
                match self.apply_connector_change(&target, change) {
                    Ok(updated) => target = updated,
                    Err(error) => {
                        Self::close_connector_popups(ctx, viewport);
                        self.toasts.warning(error);
                        return Some(toolbar.response.rect);
                    }
                }
            }
            ctx.data_mut(|data| {
                data.insert_temp(
                    popup_slot,
                    ConnectorPopups {
                        target,
                        ids: connector_popups,
                    },
                );
            });
        } else if let Some(shape) = shape.as_ref().filter(|value| value.enabled) {
            let mut target = shape.target.clone();
            if let Some(change) = shape_change {
                match self.apply_shape_change(&target, change) {
                    Ok(updated) => target = updated,
                    Err(error) => {
                        Self::close_connector_popups(ctx, viewport);
                        self.toasts.warning(error);
                        return Some(toolbar.response.rect);
                    }
                }
            }
            ctx.data_mut(|data| {
                data.insert_temp(
                    popup_slot,
                    ConnectorPopups {
                        target,
                        ids: connector_popups,
                    },
                );
            });
        }
        match context_action {
            Some(ContextAction::EditLabel) => {
                Self::close_connector_popups(ctx, viewport);
                if let Err(error) = action_target
                    .ok_or_else(|| t("Not possible here").to_owned())
                    .and_then(|target| self.edit_board_label(&target))
                {
                    self.toasts.warning(error);
                }
            }
            Some(ContextAction::FormatPanel) => command = Some(Cmd::ToggleFormatPanel),
            None => {}
        }
        if let Some(command) = command {
            self.exec(ctx, command);
        }
        if let Some(shape) = insert_shape {
            self.insert_board_shape(ctx, shape);
        }
        Some(toolbar.response.rect)
    }

    /// Shift+Enter on a library tile: places the shape in free space beside
    /// the selection (or near the viewport centre) without changing the tool.
    fn insert_board_shape(&mut self, ctx: &Context, shape: ShapeKind) {
        if !self.command_available(&Cmd::BoardTool(BoardTool::Shape(shape))) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if !self.finish_canvas_text_edit() {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        match doc.view.insert_board_shape(&mut doc.doc, shape) {
            Ok(_) => {
                ctx.memory_mut(|memory| memory.request_focus(thinoda_canvas::MapCanvas::id()));
            }
            Err(error) => {
                self.toasts
                    .error(format!("{}: {error}", t("Not possible here")));
            }
        }
    }

    /// Bottom-centre hint while a shape is armed; dismissible for the session.
    pub(crate) fn armed_shape_hint(&mut self, ctx: &Context, canvas: Rect) -> Option<Rect> {
        let doc = self.doc()?;
        if !matches!(doc.view.board_tool, BoardTool::Shape(_))
            || canvas.width() < ARMED_SHAPE_HINT_MIN_WIDTH
            || canvas.height() < 160.0
            || ctx.data(|data| data.get_temp::<bool>(armed_shape_hint_dismissed_id())) == Some(true)
        {
            return None;
        }
        let mut dismiss = false;
        let area = egui::Area::new(egui::Id::unique("thinoda_armed_shape_hint"))
            .order(egui::Order::Middle)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(egui::pos2(canvas.center().x, canvas.bottom() - 12.0))
            .constrain_to(canvas)
            .movable(false)
            .show(ctx, |ui| {
                widgets::card(ui).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        widgets::caption(ui, t(ARMED_SHAPE_HINT));
                        let close = widgets::icon_button(ui, icons::CLOSE, t("Dismiss"), false);
                        dismiss = close.clicked() && widgets::touch_activation_allowed(ctx);
                    });
                });
            });
        if dismiss {
            ctx.data_mut(|data| data.insert_temp(armed_shape_hint_dismissed_id(), true));
        }
        Some(area.response.rect)
    }

    /// Shows properties for a selected free object instead of topic styling.
    pub(crate) fn board_object_properties(&mut self, ui: &mut Ui) -> bool {
        let Some(doc) = self.doc() else { return false };
        if doc.view.board_selection.is_empty() {
            return false;
        }
        if doc.view.board_selection.len() != 1 || !doc.view.selection.is_empty() {
            widgets::caption(ui, t("Select one board object to edit its properties."));
            return true;
        }
        let id = doc.view.board_selection[0];
        let Some(original) = doc.doc.sheet().canvas.object(id).cloned() else {
            return true;
        };
        let sheet = doc.doc.sheet();
        let locked = sheet
            .expand_board_selection(&[BoardMember::Object(id)])
            .map_or(true, |members| {
                members
                    .iter()
                    .any(|member| sheet.is_board_member_locked(*member))
            });
        let inspector_owner = (
            doc.doc.instance_id(),
            doc.doc.active_index(),
            doc.doc.state_id(),
            id,
        );
        let gesture_active = doc.view.has_active_canvas_gesture();
        let mut edited = original.clone();
        let mut clear_waypoints = false;
        ui.label(
            egui::RichText::new(t(match &original.kind {
                CanvasObjectKind::Shape { shape, .. } => shape_name(*shape),
                CanvasObjectKind::Connector(_) => "Connector",
                CanvasObjectKind::Ink(_) => "Ink stroke",
            }))
            .strong(),
        );
        if locked {
            widgets::caption(ui, t("Locked"));
        }
        ui.add_enabled_ui(!locked, |ui| {
            widgets::property_field(ui, t("Description"), |ui| {
                ui.add(egui::TextEdit::multiline(&mut edited.alt_text).desired_rows(2))
            });
            widgets::property_field(ui, t("Opacity"), |ui| {
                ui.add(egui::Slider::new(&mut edited.style.opacity, 0.0..=1.0))
            });
            match &mut edited.kind {
                CanvasObjectKind::Shape { shape, .. } => {
                    widgets::property_field(ui, t("Shape"), |ui| {
                        egui::ComboBox::from_id_salt(("board_object_shape", id))
                            .selected_text(t(shape_name(*shape)))
                            .show_ui(ui, |ui| {
                                for kind in ShapeKind::ALL {
                                    ui.selectable_value(shape, kind, t(shape_name(kind)));
                                }
                            })
                            .response
                    });
                    optional_color(ui, "Fill", &mut edited.style.fill, Color::WHITE);
                    optional_color(ui, "Stroke", &mut edited.style.stroke, Color::BLACK);
                    width_field(ui, &mut edited.style.stroke_width);
                }
                CanvasObjectKind::Connector(connector) => {
                    widgets::property_field(ui, t("Connector route"), |ui| {
                        egui::ComboBox::from_id_salt(("board_connector_route", id))
                            .selected_text(t(route_name(connector.route)))
                            .show_ui(ui, |ui| {
                                for route in [
                                    ConnectorRoute::Straight,
                                    ConnectorRoute::Elbow,
                                    ConnectorRoute::Curve,
                                ] {
                                    ui.selectable_value(
                                        &mut connector.route,
                                        route,
                                        t(route_name(route)),
                                    );
                                }
                            })
                            .response
                    });
                    clear_waypoints = ui
                        .push_id(("clear-connector-waypoints", inspector_owner), |ui| {
                            ui.add_enabled(
                                !gesture_active && !connector.waypoints.is_empty(),
                                egui::Button::new(t("Clear connector waypoints")),
                            )
                            .clicked()
                        })
                        .inner;
                    let mut arrow_start = connector.arrow_start;
                    if ui.checkbox(&mut arrow_start, t("Arrow at start")).changed() {
                        connector.set_arrow_start(arrow_start);
                    }
                    marker_field(
                        ui,
                        "Start arrow style",
                        id,
                        arrow_start,
                        &mut connector.start_marker,
                        &mut connector.start_marker_size,
                        &mut connector.start_marker_unfilled,
                    );
                    marker_size_field(
                        ui,
                        "Start arrow size",
                        arrow_start && connector.start_marker.is_some(),
                        &mut connector.start_marker_size,
                    );
                    marker_fill_field(
                        ui,
                        "Fill start arrow",
                        arrow_start,
                        connector.start_marker,
                        &mut connector.start_marker_unfilled,
                    );
                    let mut arrow_end = connector.arrow_end;
                    if ui.checkbox(&mut arrow_end, t("Arrow at end")).changed() {
                        connector.set_arrow_end(arrow_end);
                    }
                    marker_field(
                        ui,
                        "End arrow style",
                        id,
                        arrow_end,
                        &mut connector.end_marker,
                        &mut connector.end_marker_size,
                        &mut connector.end_marker_unfilled,
                    );
                    marker_size_field(
                        ui,
                        "End arrow size",
                        arrow_end && connector.end_marker.is_some(),
                        &mut connector.end_marker_size,
                    );
                    marker_fill_field(
                        ui,
                        "Fill end arrow",
                        arrow_end,
                        connector.end_marker,
                        &mut connector.end_marker_unfilled,
                    );
                    optional_color(ui, "Stroke", &mut connector.color, Color::BLACK);
                    width_field(ui, &mut connector.width);
                }
                CanvasObjectKind::Ink(ink) => {
                    color_field(ui, "Stroke", &mut ink.color);
                    width_field(ui, &mut ink.width);
                }
            }
        });
        if edited != original || clear_waypoints {
            let result = (|| {
                let doc = self
                    .doc_mut()
                    .ok_or_else(|| t("Not possible here").to_owned())?;
                doc.commit_canvas_drafts()?;
                doc.doc
                    .update_canvas_object("Board properties", id, |current| {
                        current.alt_text = edited.alt_text;
                        current.style = edited.style;
                        match (&mut current.kind, edited.kind) {
                            (
                                CanvasObjectKind::Shape { shape, .. },
                                CanvasObjectKind::Shape { shape: value, .. },
                            ) => *shape = value,
                            (
                                CanvasObjectKind::Connector(current),
                                CanvasObjectKind::Connector(value),
                            ) => {
                                current.route = value.route;
                                current.arrow_start = value.arrow_start;
                                current.arrow_end = value.arrow_end;
                                current.start_marker = value.start_marker;
                                current.end_marker = value.end_marker;
                                current.start_marker_size = value.start_marker_size;
                                current.end_marker_size = value.end_marker_size;
                                current.start_marker_unfilled = value.start_marker_unfilled;
                                current.end_marker_unfilled = value.end_marker_unfilled;
                                current.color = value.color;
                                current.width = value.width;
                                if clear_waypoints {
                                    current.waypoints = Default::default();
                                }
                            }
                            (CanvasObjectKind::Ink(current), CanvasObjectKind::Ink(value)) => {
                                current.color = value.color;
                                current.width = value.width;
                            }
                            _ => {}
                        }
                    })
                    .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                self.toasts.error(error);
            }
        }
        true
    }

    pub(crate) fn board_card_properties(&mut self, ui: &mut Ui) {
        let Some(doc) = self.doc() else { return };
        if doc.view.selection.len() != 1 || !doc.view.board_selection.is_empty() {
            return;
        }
        let id = doc.view.selection[0];
        let Some(card) = doc.doc.sheet().topic(id).and_then(|topic| topic.card) else {
            return;
        };
        let sheet = doc.doc.sheet();
        let locked = sheet
            .expand_board_selection(&[BoardMember::Card(id)])
            .map_or(true, |members| {
                members
                    .iter()
                    .any(|member| sheet.is_board_member_locked(*member))
            });
        let mut edited = card;
        widgets::section(ui, t("Card"), true, |ui| {
            if locked {
                widgets::caption(ui, t("Locked"));
            }
            ui.add_enabled_ui(!locked, |ui| {
                optional_color(ui, "Fill", &mut edited.color, Color::WHITE);
                for (label, value) in [
                    ("Width", &mut edited.size.x),
                    ("Height", &mut edited.size.y),
                ] {
                    widgets::property_field(ui, t(label), |ui| {
                        ui.add(
                            egui::DragValue::new(value).range(MIN_CARD_SIZE..=MAX_BOARD_COORDINATE),
                        )
                    });
                }
                widgets::property_field(ui, t("Rotation"), |ui| {
                    ui.add(egui::DragValue::new(&mut edited.rotation).range(0.0..=359.9))
                });
            });
        });
        if edited != card {
            let result = (|| {
                let doc = self
                    .doc_mut()
                    .ok_or_else(|| t("Not possible here").to_owned())?;
                doc.commit_canvas_drafts()?;
                doc.doc
                    .edit_sheet("Card properties", |sheet| {
                        sheet
                            .set_topic_card(id, Some(edited))
                            .map_err(thinoda_core::EditError::Invalid)
                    })
                    .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                self.toasts.error(error);
            }
        }
    }
}

fn board_tool_button(
    ui: &mut Ui,
    icon: CommandIcon,
    label: &'static str,
    selected: bool,
    collapsed: bool,
    row_height: f32,
) -> (egui::Response, bool) {
    let detail = if collapsed {
        Detail::CompactIcon
    } else {
        Detail::CompactLabeled
    };
    let width = ui
        .available_width()
        .min(if collapsed { 34.0 } else { 200.0 });
    ribbon::command_button_with_size(
        ui,
        icon,
        t(label),
        detail,
        selected,
        None,
        vec2(width, row_height),
    )
}

fn board_tool_popup(
    ui: &mut Ui,
    icon: CommandIcon,
    label: &'static str,
    collapsed: bool,
    selected: bool,
    row_height: f32,
    content: impl FnOnce(&mut Ui),
) -> egui::Response {
    let (response, activated) = board_tool_button(ui, icon, label, selected, collapsed, row_height);
    let popup = egui::Popup::menu(&response)
        .accessible_name(t(label))
        .open_memory(None)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let id = popup.get_id();
    if activated {
        if popup.is_open() {
            egui::Popup::close_id(ui.ctx(), id);
        } else {
            egui::Popup::open_id(ui.ctx(), id);
        }
    }
    let escape = popup.is_open() && ui.input(|input| input.key_pressed(egui::Key::Escape));
    popup.show(content);
    if escape {
        egui::Popup::close_id(ui.ctx(), id);
        ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
        });
        response.request_focus();
    }
    let open = egui::Popup::is_id_open(ui.ctx(), id);
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_has_popup(egui::accesskit::HasPopup::Menu);
        node.set_expanded(open);
    });
    response
}

fn connector_context_controls(
    ui: &mut Ui,
    canvas: Rect,
    collapsed: bool,
    row_height: f32,
    context: &ConnectorContext,
) -> (Option<ConnectorChange>, Vec<egui::Id>) {
    let properties = context.properties;
    let mut change = None;
    let mut popups = Vec::with_capacity(3);
    let route = board_tool_popup(
        ui,
        icons::BRANCH.into(),
        "Connector route",
        collapsed,
        false,
        row_height,
        |ui| {
            ui.set_width((canvas.width() - 24.0).clamp(1.0, 170.0));
            reserve_popup_scrollbar_space(ui);
            let max_height = (canvas.height() - 32.0).clamp(1.0, 180.0);
            egui::ScrollArea::vertical()
                .id_salt("connector_route_choices")
                .min_scrolled_height(0.0)
                .max_height(max_height)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for route in [
                            ConnectorRoute::Straight,
                            ConnectorRoute::Elbow,
                            ConnectorRoute::Curve,
                        ] {
                            if connector_route_tile(ui, route, properties.route)
                                && route != properties.route
                            {
                                change = Some(ConnectorChange::Route(route));
                            }
                        }
                    });
                    if ui
                        .add_enabled(
                            properties.has_waypoints,
                            egui::Button::new(t("Clear connector waypoints")),
                        )
                        .clicked()
                    {
                        change = Some(ConnectorChange::ClearWaypoints);
                    }
                    crate::ui::reveal_changed_focus(ui);
                });
        },
    );
    popups.push(egui::Popup::menu(&route).get_id());
    if board_tool_button(
        ui,
        icons::ARROW_LEFT.into(),
        "Arrow at start",
        properties.arrow_start,
        collapsed,
        row_height,
    )
    .1
    {
        change = Some(ConnectorChange::ArrowStart(!properties.arrow_start));
    }
    if board_tool_button(
        ui,
        icons::ARROW_RIGHT.into(),
        "Arrow at end",
        properties.arrow_end,
        collapsed,
        row_height,
    )
    .1
    {
        change = Some(ConnectorChange::ArrowEnd(!properties.arrow_end));
    }
    let (color, picked) = color_popup(
        ui,
        canvas,
        collapsed,
        row_height,
        CONNECTOR_COLOR,
        &context.target,
        properties.color,
        properties.effective_color,
        Some(properties.effective_color),
    );
    popups.push(color);
    if let Some(color) = picked {
        change = Some(ConnectorChange::Color(color));
    }
    let (width, picked) = width_popup(
        ui,
        canvas,
        collapsed,
        row_height,
        CONNECTOR_WIDTH,
        &context.target,
        properties.width,
    );
    popups.push(width);
    if let Some(width) = picked {
        change = Some(ConnectorChange::Width(width));
    }
    (change, popups)
}

fn shape_context_controls(
    ui: &mut Ui,
    canvas: Rect,
    collapsed: bool,
    row_height: f32,
    context: &ShapeContext,
) -> (Option<ShapeChange>, Vec<egui::Id>) {
    let properties = context.properties;
    let mut change = None;
    let mut popups = Vec::with_capacity(4);
    let shape = board_tool_popup(
        ui,
        icons::BOARD_SHAPES.into(),
        "Change shape",
        collapsed,
        false,
        row_height,
        |ui| {
            if let Some(shape) = shape_library(ui, canvas, Some(properties.shape), t)
                && shape != properties.shape
            {
                change = Some(ShapeChange::Shape(shape));
            }
        },
    );
    popups.push(egui::Popup::menu(&shape).get_id());
    let (fill, picked) = color_popup(
        ui,
        canvas,
        collapsed,
        row_height,
        SHAPE_FILL,
        &context.target,
        properties.fill,
        properties.fill.unwrap_or(Color::WHITE),
        properties.fill,
    );
    popups.push(fill);
    if let Some(color) = picked {
        change = Some(ShapeChange::Fill(color));
    }
    let (outline, picked) = color_popup(
        ui,
        canvas,
        collapsed,
        row_height,
        SHAPE_OUTLINE,
        &context.target,
        properties.stroke,
        properties.stroke.unwrap_or(properties.auto_stroke),
        properties.visible_stroke(),
    );
    popups.push(outline);
    if let Some(color) = picked {
        change = Some(ShapeChange::Stroke(color));
    }
    let (width, picked) = width_popup(
        ui,
        canvas,
        collapsed,
        row_height,
        SHAPE_OUTLINE_WIDTH,
        &context.target,
        properties.stroke_width,
    );
    popups.push(width);
    if let Some(width) = picked {
        change = Some(ShapeChange::StrokeWidth(width));
    }
    (change, popups)
}

/// "Edit label" and "More formatting options" shortcuts shared by shapes and connectors.
fn context_action_controls(
    ui: &mut Ui,
    collapsed: bool,
    row_height: f32,
    enabled: bool,
    format_panel_open: bool,
) -> Option<ContextAction> {
    let mut action = None;
    ui.add_enabled_ui(enabled, |ui| {
        if board_tool_button(
            ui,
            icons::EDIT.into(),
            "Edit label",
            false,
            collapsed,
            row_height,
        )
        .1
        {
            action = Some(ContextAction::EditLabel);
        }
    });
    if board_tool_button(
        ui,
        icons::TUNE.into(),
        "More formatting options",
        format_panel_open,
        collapsed,
        row_height,
    )
    .1
    {
        action = Some(ContextAction::FormatPanel);
    }
    action
}

/// Board-toolbar color menu: a "none" choice, swatches and a custom picker.
#[allow(clippy::too_many_arguments)]
fn color_popup(
    ui: &mut Ui,
    canvas: Rect,
    collapsed: bool,
    row_height: f32,
    control: ColorControl,
    target: &ConnectorTarget,
    current: Option<Color>,
    picker_start: Color,
    swatch: Option<Color>,
) -> (egui::Id, Option<Option<Color>>) {
    let mut change = None;
    let response = board_tool_popup(
        ui,
        control.icon.into(),
        control.label,
        collapsed,
        false,
        row_height,
        |ui| {
            ui.set_width((canvas.width() - 24.0).clamp(1.0, 275.0));
            reserve_popup_scrollbar_space(ui);
            let max_height = (canvas.height() - 32.0).clamp(1.0, 180.0);
            egui::ScrollArea::vertical()
                .id_salt((control.key, "color_choices"))
                .min_scrolled_height(0.0)
                .max_height(max_height)
                .show(ui, |ui| {
                    let none = ui
                        .selectable_label(current.is_none(), t(control.none))
                        .accessible_name(format!("{} · {}", t(control.label), t(control.none)));
                    if none.clicked()
                        && widgets::touch_activation_allowed(ui.ctx())
                        && current.is_some()
                    {
                        change = Some(None);
                    }
                    let selected =
                        current.map(|color| Color32::from_rgb(color.r, color.g, color.b));
                    if let Some(color) = widgets::color_swatches(ui, &crate::ui::SWATCHES, selected)
                    {
                        let [r, g, b, _] = color.to_srgba_unmultiplied();
                        let alpha = current.map_or(255, |color| color.a);
                        let color = Some(Color::rgba(r, g, b, alpha));
                        if color != current {
                            change = Some(color);
                        }
                    }
                    let custom = ui.collapsing(t("Custom color"), |ui| {
                        ui.spacing_mut().slider_width = ui.available_width().min(260.0);
                        if let Some(color) =
                            custom_color_picker(ui, control.key, target, picker_start)
                            && Some(color) != current
                        {
                            change = Some(Some(color));
                        }
                    });
                    if custom.header_response.clicked() && custom.body_response.is_some() {
                        custom.header_response.scroll_to_me(Some(egui::Align::Min));
                    }
                    crate::ui::reveal_changed_focus(ui);
                });
        },
    );
    let rect = Rect::from_min_size(
        response.rect.left_bottom() + vec2(8.0, -6.0),
        vec2(16.0, 3.0),
    );
    let border = thinoda_ui::Palette::of_ui(ui).border;
    if let Some(color) = swatch {
        ui.painter()
            .rect_filled(rect, 1, crate::ui::from_color(color));
    } else {
        // A diagonal slash marks "no color", as in common design tools.
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_top()],
            egui::Stroke::new(1.0, border),
        );
    }
    ui.painter().rect_stroke(
        rect,
        1,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    (egui::Popup::menu(&response).get_id(), change)
}

/// Board-toolbar width menu: presets plus an exact field committed on Enter or focus loss.
fn width_popup(
    ui: &mut Ui,
    canvas: Rect,
    collapsed: bool,
    row_height: f32,
    control: WidthControl,
    target: &ConnectorTarget,
    current: f32,
) -> (egui::Id, Option<f32>) {
    let mut change = None;
    let response = board_tool_popup(
        ui,
        icons::STROKE_WIDTH.into(),
        control.label,
        collapsed,
        false,
        row_height,
        |ui| {
            ui.set_width((canvas.width() - 24.0).clamp(1.0, 206.0));
            reserve_popup_scrollbar_space(ui);
            let max_height = (canvas.height() - 32.0).clamp(1.0, 180.0);
            egui::ScrollArea::vertical()
                .id_salt((control.key, "width_choices"))
                .min_scrolled_height(0.0)
                .max_height(max_height)
                .show(ui, |ui| {
                    let mut width = current;
                    width_presets(ui, &mut width, control.label);
                    let preset_changed = width != current;
                    if preset_changed {
                        change = Some(width);
                    }
                    let id = ui.make_persistent_id((control.key, "width_text_draft"));
                    let frame = ui.ctx().cumulative_frame_nr();
                    let allowed = widgets::touch_activation_allowed(ui.ctx());
                    let owner = egui::Id::unique((control.key, "width", target));
                    let pending = ui.data(|data| data.get_temp::<WidthDraft>(id));
                    let pending = pending.filter(|draft| {
                        allowed && draft.owner == owner && frame <= draft.frame.saturating_add(1)
                    });
                    if !preset_changed && let Some(draft) = pending {
                        width = draft.value;
                    }
                    let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
                    let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                    let (response, commit) = crate::ui::touch_numeric_value(
                        ui,
                        &mut width,
                        control.salt,
                        Some(owner),
                        |ui, value| {
                            ui.add(
                                egui::DragValue::new(value)
                                    .range(
                                        thinoda_core::board::MIN_STROKE_WIDTH
                                            ..=thinoda_core::board::MAX_STROKE_WIDTH,
                                    )
                                    .speed(0.5),
                            )
                            .accessible_name(t(control.label))
                        },
                    );
                    if response.gained_focus() {
                        response.scroll_to_me(None);
                    }
                    if response.lost_focus() && ui.is_enabled() {
                        ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
                        });
                    }
                    let editing = response.has_focus() && ui.ctx().text_edit_focused();
                    if preset_changed || escape || !allowed {
                        ui.data_mut(|data| data.remove::<WidthDraft>(id));
                    } else if editing && !enter {
                        if commit || pending.is_some() {
                            ui.data_mut(|data| {
                                data.insert_temp(
                                    id,
                                    WidthDraft {
                                        owner,
                                        value: width,
                                        frame,
                                    },
                                );
                            });
                        }
                    } else {
                        ui.data_mut(|data| data.remove::<WidthDraft>(id));
                        if (commit || pending.is_some() && (enter || response.lost_focus()))
                            && width != current
                            && width.is_finite()
                        {
                            change = Some(width);
                        }
                    }
                    crate::ui::reveal_changed_focus(ui);
                });
        },
    );
    (egui::Popup::menu(&response).get_id(), change)
}

fn connector_route_tile(ui: &mut Ui, route: ConnectorRoute, current: ConnectorRoute) -> bool {
    let selected = route == current;
    let label = t(route_name(route));
    let response = ui.add_sized(vec2(46.0, 36.0), egui::Button::new("").selected(selected));
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    if response.gained_focus() {
        response.scroll_to_me(None);
    }
    if ui.is_rect_visible(response.rect) {
        let rect = response.rect.shrink(8.0);
        let start = egui::pos2(rect.left(), rect.bottom());
        let end = egui::pos2(rect.right(), rect.top());
        let points = match route {
            ConnectorRoute::Straight => vec![start, end],
            ConnectorRoute::Elbow => vec![
                start,
                egui::pos2(rect.center().x, start.y),
                egui::pos2(rect.center().x, end.y),
                end,
            ],
            ConnectorRoute::Curve => (0..=16)
                .map(|step| {
                    let t = step as f32 / 16.0;
                    let x = egui::lerp(start.x..=end.x, t);
                    let smooth = t * t * (3.0 - 2.0 * t);
                    egui::pos2(x, egui::lerp(start.y..=end.y, smooth))
                })
                .collect(),
        };
        let palette = thinoda_ui::Palette::of_ui(ui);
        let color = if !ui.is_enabled() {
            palette.text_muted
        } else if selected {
            palette.accent
        } else {
            palette.text
        };
        ui.painter()
            .add(egui::Shape::line(points, egui::Stroke::new(2.0, color)));
    }
    let keyboard = ui.is_enabled()
        && response.has_focus()
        && ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || input.consume_key(egui::Modifiers::NONE, egui::Key::Space)
        });
    let activated = ui.is_enabled()
        && ((widgets::touch_activation_allowed(ui.ctx()) && response.clicked()) || keyboard);
    response.on_hover_text(label);
    activated
}

fn custom_color_picker(
    ui: &mut Ui,
    key: &'static str,
    target: &ConnectorTarget,
    start: Color,
) -> Option<Color> {
    let id = ui.make_persistent_id((key, "color_draft"));
    let owner = egui::Id::unique((key, "color", target));
    let frame = ui.ctx().cumulative_frame_nr();
    let pending = ui.data(|data| data.get_temp::<ColorDraft>(id));
    let allowed = widgets::touch_activation_allowed(ui.ctx());
    let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
    let canceled = !ui.is_enabled()
        || !allowed
        || ui.input(|input| input.key_pressed(egui::Key::Escape))
        || pending.is_some_and(|draft| {
            draft.canceled || draft.owner != owner || frame > draft.frame.saturating_add(1)
        });
    let mut value = pending.filter(|_| !canceled).map_or_else(
        || {
            let color = start;
            let mut value = egui::ecolor::Hsva::from_srgb([color.r, color.g, color.b]);
            value.a = f32::from(color.a) / 255.0;
            value
        },
        |draft| draft.value,
    );
    let changed = egui::color_picker::color_picker_hsva_2d(
        ui,
        &mut value,
        egui::color_picker::Alpha::OnlyBlend,
    );
    let pointer_down = ui.input(|input| input.pointer.primary_down());
    let editing = ui.ctx().text_edit_focused()
        && ui
            .memory(|memory| memory.focused())
            .and_then(|focused| ui.ctx().read_response(focused))
            .is_some_and(|response| crate::ui::response_is_within(ui, &response));
    if canceled {
        ui.data_mut(|data| {
            if pointer_down {
                data.insert_temp(
                    id,
                    ColorDraft {
                        owner,
                        value,
                        frame,
                        canceled: true,
                    },
                );
            } else {
                data.remove::<ColorDraft>(id);
            }
        });
        return None;
    }
    if pointer_down || editing && !enter {
        if changed || pending.is_some() {
            ui.data_mut(|data| {
                data.insert_temp(
                    id,
                    ColorDraft {
                        owner,
                        value,
                        frame,
                        canceled: false,
                    },
                );
            });
        }
        return None;
    }
    if changed || pending.is_some() {
        ui.data_mut(|data| data.remove::<ColorDraft>(id));
        let [r, g, b, a] = value.to_srgba_unmultiplied();
        return Some(Color::rgba(r, g, b, a));
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShapePick {
    /// Click, tap, Enter or Space.
    Choose(ShapeKind),
    /// Shift+Enter on a tile of the inserting library.
    Insert(ShapeKind),
    /// A tile started dragging a [`thinoda_canvas::ShapeDrop`] payload.
    Dragged,
}

/// Searchable shape grid that only picks a shape (e.g. Change shape).
fn shape_library(
    ui: &mut Ui,
    canvas: Rect,
    current: Option<ShapeKind>,
    translate: fn(&'static str) -> &'static str,
) -> Option<ShapeKind> {
    match shape_grid(ui, canvas, current, translate, false)? {
        ShapePick::Choose(shape) => Some(shape),
        ShapePick::Insert(_) | ShapePick::Dragged => None,
    }
}

/// Shape grid; with `inserting`, mouse and pen drag tiles onto the board and
/// Shift+Enter inserts the focused tile's shape.
fn shape_grid(
    ui: &mut Ui,
    canvas: Rect,
    current: Option<ShapeKind>,
    translate: fn(&'static str) -> &'static str,
    inserting: bool,
) -> Option<ShapePick> {
    const TILE: f32 = 44.0;
    const GAP: f32 = 4.0;
    const COLUMNS: f32 = 6.0;

    reserve_popup_scrollbar_space(ui);
    let available = canvas.size().min(ui.ctx().content_rect().size());
    let frame = egui::Frame::popup(ui.style()).total_margin().sum();
    let width = (available.x - frame.x - 8.0)
        .max(1.0)
        .min(COLUMNS * TILE + (COLUMNS - 1.0) * GAP + ui.spacing().scroll.allocated_width());
    ui.set_width(width);

    let top = ui.cursor().min.y;
    let id = ui.make_persistent_id("board_shape_search");
    let mut query = ui.data(|data| data.get_temp::<String>(id).unwrap_or_default());
    let search = ui
        .add(
            egui::TextEdit::singleline(&mut query)
                .hint_text(translate("Search shapes"))
                .desired_width(width),
        )
        .accessible_name(translate("Search shapes"));
    ui.data_mut(|data| data.insert_temp(id, query.clone()));
    // The hint is supplementary; tiny popups keep their rows for the tiles.
    if inserting && width >= 160.0 && available.y >= 240.0 {
        widgets::caption(
            ui,
            translate("Drag onto the board or press Shift+Enter to insert"),
        );
    }
    let query = query.to_lowercase();
    let shapes: Vec<_> = ShapeKind::ALL
        .into_iter()
        .filter_map(|shape| {
            let name = translate(shape_name(shape));
            (query.is_empty() || name.to_lowercase().contains(&query)).then_some((shape, name))
        })
        .collect();

    let height = (available.y - frame.y - (ui.cursor().min.y - top) - 8.0).clamp(0.0, 280.0);
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("board_shape_grid")
        .min_scrolled_height(0.0)
        .max_height(height);
    if search.changed() {
        scroll = scroll.vertical_scroll_offset(0.0);
    }

    let mut selected = None;
    scroll.show(ui, |ui| {
        if shapes.is_empty() {
            widgets::caption(ui, translate("No results"));
        } else {
            let tile_size = TILE.min(ui.available_width()).min(height).max(1.0);
            let columns = ((ui.available_width() + GAP) / (tile_size + GAP))
                .floor()
                .max(1.0) as usize;
            ui.spacing_mut().interact_size.y = tile_size;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                let mut focused = None;
                let mut ids = Vec::with_capacity(shapes.len());
                for (index, (shape, name)) in shapes.iter().copied().enumerate() {
                    let (response, pick) = shape_tile(
                        ui,
                        shape,
                        name,
                        current == Some(shape),
                        tile_size,
                        inserting,
                    );
                    ids.push(response.id);
                    if response.has_focus() {
                        focused = Some(index);
                    }
                    selected = selected.or(pick);
                }
                if let Some(index) = focused {
                    let direction = ui.input_mut(|input| {
                        [
                            egui::Key::ArrowLeft,
                            egui::Key::ArrowRight,
                            egui::Key::ArrowUp,
                            egui::Key::ArrowDown,
                        ]
                        .into_iter()
                        .find(|key| input.consume_key(egui::Modifiers::NONE, *key))
                    });
                    if let Some(direction) = direction {
                        let next = match direction {
                            egui::Key::ArrowLeft if index % columns != 0 => Some(index - 1),
                            egui::Key::ArrowRight
                                if index % columns + 1 < columns && index + 1 < ids.len() =>
                            {
                                Some(index + 1)
                            }
                            egui::Key::ArrowUp => index.checked_sub(columns),
                            egui::Key::ArrowDown if index + columns < ids.len() => {
                                Some(index + columns)
                            }
                            _ => None,
                        };
                        ui.memory_mut(|memory| {
                            memory.move_focus(egui::FocusDirection::None);
                            if let Some(next) = next {
                                memory.request_focus(ids[next]);
                            }
                        });
                        ui.ctx().request_repaint();
                    }
                }
            });
        }
    });
    selected
}

fn shape_tile(
    ui: &mut Ui,
    shape: ShapeKind,
    name: &str,
    selected: bool,
    size: f32,
    inserting: bool,
) -> (egui::Response, Option<ShapePick>) {
    let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
    let palette = thinoda_ui::Palette::of_ui(ui);
    let (_, rect) = ui.allocate_space(egui::Vec2::splat(size));
    let id = ui.make_persistent_id(("board_shape_tile", shape.id()));
    // Touch swipes keep scrolling the grid instead of dragging a tile.
    let sense = if inserting && !ui.input(|input| input.any_touches()) {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::click()
    };
    let response = ui.interact(rect, id, sense);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, name)
    });
    if response.gained_focus() {
        response.scroll_to_me(None);
    }
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() || response.is_pointer_button_down_on();
        let fill = if selected {
            palette.accent_soft
        } else if hovered {
            ui.style().interact(&response).bg_fill
        } else {
            palette.surface_alt
        };
        let border = if selected {
            palette.accent
        } else {
            palette.border
        };
        ui.painter().rect(
            rect.shrink(0.5),
            7,
            fill,
            egui::Stroke::new(1.0, border),
            egui::StrokeKind::Inside,
        );
        if response.has_focus() && ui.is_enabled() {
            ui.painter().rect_stroke(
                rect.shrink(2.0),
                6,
                egui::Stroke::new(2.0, palette.text),
                egui::StrokeKind::Inside,
            );
        }
        let color = if selected {
            palette.accent
        } else {
            palette.text
        };
        let icon = Rect::from_center_size(rect.center(), vec2(size * 0.62, size * 0.52));
        let point =
            |p: thinoda_core::Vec2| icon.min + vec2(p.x * icon.width(), p.y * icon.height());
        let stroke = egui::Stroke::new(1.5, color);
        ui.painter().add(egui::Shape::closed_line(
            shape.unit_outline().into_iter().map(point).collect(),
            stroke,
        ));
        for detail in shape.unit_details() {
            ui.painter().add(egui::Shape::line(
                detail.into_iter().map(point).collect(),
                stroke,
            ));
        }
    }
    if inserting && ui.is_enabled() && response.drag_started() {
        response.dnd_set_drag_payload(thinoda_canvas::ShapeDrop(shape));
        return (response, Some(ShapePick::Dragged));
    }
    // egui also reports Shift+Enter as a click, so take it first.
    let insert = inserting
        && ui.is_enabled()
        && response.has_focus()
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter));
    if insert {
        return (response.on_hover_text(name), Some(ShapePick::Insert(shape)));
    }
    let keyboard = ui.is_enabled()
        && response.has_focus()
        && ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || input.consume_key(egui::Modifiers::NONE, egui::Key::Space)
        });
    let activated = ui.is_enabled() && ((touch_allowed && response.clicked()) || keyboard);
    (
        response.on_hover_text(name),
        activated.then_some(ShapePick::Choose(shape)),
    )
}

fn reserve_popup_scrollbar_space(ui: &mut Ui) {
    let scroll = &mut ui.spacing_mut().scroll;
    if scroll.floating {
        // Floating bars otherwise intercept clicks on controls under their full hit area.
        scroll.floating_allocated_width = scroll
            .floating_allocated_width
            .max(scroll.bar_width + scroll.bar_outer_margin);
    }
}

fn width_presets(ui: &mut Ui, current: &mut f32, field: &'static str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for width in [1.0, 2.0, 4.0, 8.0, 16.0] {
            let selected = (*current - width).abs() < f32::EPSILON;
            let label = format!("{}: {width}", t(field));
            let response = ui
                .add_sized(vec2(38.0, 32.0), egui::Button::new("").selected(selected))
                .on_hover_text(&label);
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, &label)
            });
            let center = response.rect.center();
            ui.painter().line_segment(
                [center - vec2(11.0, 0.0), center + vec2(11.0, 0.0)],
                egui::Stroke::new(width, thinoda_ui::Palette::of_ui(ui).text),
            );
            if widgets::touch_activation_allowed(ui.ctx()) && response.clicked() {
                *current = width;
            }
        }
    });
}

fn route_name(route: ConnectorRoute) -> &'static str {
    match route {
        ConnectorRoute::Straight => "Straight",
        ConnectorRoute::Elbow => "Elbow",
        ConnectorRoute::Curve => "Curve",
    }
}

fn marker_field(
    ui: &mut Ui,
    label: &'static str,
    id: CanvasObjectId,
    enabled: bool,
    marker: &mut Option<ConnectorMarker>,
    size: &mut Option<f32>,
    unfilled: &mut bool,
) {
    let name = |marker: Option<ConnectorMarker>| match marker.map(|marker| marker.kind) {
        None => "Legacy triangle",
        Some(ConnectorMarkerKind::Classic) => "Classic arrow",
        Some(ConnectorMarkerKind::ClassicThin) => "Thin classic arrow",
        Some(ConnectorMarkerKind::Open) => "Open arrow",
        Some(ConnectorMarkerKind::OpenThin) => "Thin open arrow",
        Some(ConnectorMarkerKind::Block) => "Block arrow",
        Some(ConnectorMarkerKind::BlockThin) => "Thin block arrow",
        Some(ConnectorMarkerKind::Diamond) => "Diamond",
        Some(ConnectorMarkerKind::DiamondThin) => "Thin diamond",
        Some(ConnectorMarkerKind::Oval) => "Oval",
        Some(ConnectorMarkerKind::HalfCircle) => "Half circle",
    };
    ui.add_enabled_ui(enabled, |ui| {
        widgets::property_field(ui, t(label), |ui| {
            egui::ComboBox::from_id_salt(("board_connector_marker", id, label))
                .selected_text(t(name(*marker)))
                .show_ui(ui, |ui| {
                    for choice in [
                        None,
                        Some(ConnectorMarker {
                            kind: ConnectorMarkerKind::Classic,
                        }),
                        Some(ConnectorMarker {
                            kind: ConnectorMarkerKind::ClassicThin,
                        }),
                        Some(ConnectorMarker {
                            kind: ConnectorMarkerKind::Open,
                        }),
                        Some(ConnectorMarker {
                            kind: ConnectorMarkerKind::OpenThin,
                        }),
                        Some(ConnectorMarker {
                            kind: ConnectorMarkerKind::Block,
                        }),
                        Some(ConnectorMarker {
                            kind: ConnectorMarkerKind::BlockThin,
                        }),
                        Some(ConnectorMarker {
                            kind: ConnectorMarkerKind::Diamond,
                        }),
                        Some(ConnectorMarker {
                            kind: ConnectorMarkerKind::DiamondThin,
                        }),
                        Some(ConnectorMarker {
                            kind: ConnectorMarkerKind::Oval,
                        }),
                        Some(ConnectorMarker {
                            kind: ConnectorMarkerKind::HalfCircle,
                        }),
                    ] {
                        ui.selectable_value(marker, choice, t(name(choice)));
                    }
                })
                .response
        });
    });
    if marker.is_none() {
        *size = None;
        *unfilled = false;
    }
}

fn marker_size_field(ui: &mut Ui, label: &'static str, enabled: bool, size: &mut Option<f32>) {
    let mut value = size.unwrap_or(DEFAULT_CONNECTOR_MARKER_SIZE);
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            widgets::property_field(ui, t(label), |ui| {
                ui.add(
                    egui::DragValue::new(&mut value)
                        .range(0.0..=MAX_CONNECTOR_MARKER_SIZE)
                        .speed(0.1),
                )
            })
        })
        .inner;
    if enabled && response.changed() {
        *size = (value != DEFAULT_CONNECTOR_MARKER_SIZE).then_some(value);
    }
    if !enabled {
        response.on_disabled_hover_text(t(
            "Enable an arrow and select a draw.io arrow style to set its size.",
        ));
    }
}

fn marker_fill_field(
    ui: &mut Ui,
    label: &'static str,
    enabled: bool,
    marker: Option<ConnectorMarker>,
    unfilled: &mut bool,
) {
    let open = marker.is_some_and(|marker| {
        matches!(
            marker.kind,
            ConnectorMarkerKind::Open
                | ConnectorMarkerKind::OpenThin
                | ConnectorMarkerKind::HalfCircle
        )
    });
    let can_fill = enabled && marker.is_some() && !open;
    let mut filled = !*unfilled;
    let response = ui.add_enabled(can_fill, egui::Checkbox::new(&mut filled, t(label)));
    if response.changed() {
        *unfilled = !filled;
    }
    if !enabled || marker.is_none() {
        response.on_disabled_hover_text(t(
            "Enable an arrow and select a draw.io arrow style to set its fill.",
        ));
    } else if open {
        response.on_disabled_hover_text(t(
            "Open markers cannot be filled. The fill setting is retained when changing to a fillable marker.",
        ));
    }
}

fn width_field(ui: &mut Ui, width: &mut f32) {
    widgets::property_field(ui, t("Width"), |ui| {
        ui.add(egui::DragValue::new(width).range(0.1..=200.0))
    });
}

fn color_field(ui: &mut Ui, label: &'static str, color: &mut Color) {
    let mut value = Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
    if widgets::property_field(ui, t(label), |ui| ui.color_edit_button_srgba(&mut value)).changed()
    {
        let [r, g, b, a] = value.to_srgba_unmultiplied();
        *color = Color::rgba(r, g, b, a);
    }
}

fn optional_color(ui: &mut Ui, label: &'static str, color: &mut Option<Color>, fallback: Color) {
    let mut custom = color.is_some();
    widgets::property_field(ui, t(label), |ui| {
        ui.checkbox(&mut custom, t("Custom color"))
    });
    if custom {
        let mut value = color.unwrap_or(fallback);
        color_field(ui, label, &mut value);
        *color = Some(value);
    } else {
        *color = None;
    }
}

#[cfg(test)]
#[path = "connector_context_tests.rs"]
mod connector_context_tests;

#[cfg(test)]
#[path = "shape_context_tests.rs"]
mod shape_context_tests;

#[cfg(test)]
#[path = "shape_insert_tests.rs"]
mod shape_insert_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    use thinoda_core::{
        CanvasConnector, CanvasObject, Endpoint, InkStroke, Rect as BoardRect, Vec2,
        board::{Card, CardKind},
    };

    fn start_expanded(ctx: &Context, app: &ThinodaApp) {
        let id = egui::Id::unique(("board_toolbar_collapsed", &app.doc().unwrap().key));
        ctx.data_mut(|data| {
            if data.get_temp::<bool>(id).is_none() {
                data.insert_temp(id, false);
            }
        });
    }

    fn expanded_toolbar(ui: &mut Ui, app: &mut ThinodaApp) {
        start_expanded(ui.ctx(), app);
        app.board_toolbar(ui.ctx(), ui.max_rect(), None);
    }

    fn shape_test_theme(ctx: &Context, dark: bool) {
        thinoda_ui::theme::install(
            ctx,
            if dark {
                thinoda_ui::theme::ThemeMode::Dark
            } else {
                thinoda_ui::theme::ThemeMode::Light
            },
            thinoda_ui::tokens::DEFAULT_ACCENT,
        );
    }

    fn harness() -> Harness<'static, ThinodaApp> {
        let mut harness = Harness::builder()
            .with_size(vec2(1400.0, 900.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    start_expanded(ui.ctx(), app);
                    app.draw(ui);
                },
                ThinodaApp::headless(),
            );
        harness.run_steps(3);
        harness
    }

    fn click_scrolled_into_view(harness: &mut Harness<'_, ThinodaApp>, label: &str) {
        harness.get_by_label(label).scroll_to_me();
        harness.run_steps(4);
        harness.get_by_label(label).click();
        harness.run_steps(2);
    }

    fn click_point(harness: &mut Harness<'_, ThinodaApp>, pos: egui::Pos2) {
        harness.event(egui::Event::PointerMoved(pos));
        harness.run_steps(1);
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run_steps(1);
        }
        harness.run_steps(2);
    }

    #[test]
    fn maps_and_board_documents_start_collapsed_and_can_expand() {
        let mut legacy = Harness::builder().build_ui_state(
            |ui, app: &mut ThinodaApp| {
                app.board_toolbar(ui.ctx(), ui.max_rect(), None);
            },
            ThinodaApp::headless(),
        );
        legacy.run_steps(3);
        assert!(legacy.query_by_label("Expand board toolbar").is_some());
        legacy.get_by_label("Expand board toolbar").click();
        legacy.run_steps(2);
        assert!(legacy.query_by_label("Collapse board toolbar").is_some());

        let mut board = ThinodaApp::headless();
        board
            .doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Board grid", |sheet| {
                sheet.board.grid = thinoda_core::GridKind::Dots;
                Ok(())
            })
            .unwrap();
        let mut board = Harness::builder().build_ui_state(
            |ui, app: &mut ThinodaApp| {
                app.board_toolbar(ui.ctx(), ui.max_rect(), None);
            },
            board,
        );
        board.run_steps(3);
        assert!(board.query_by_label("Expand board toolbar").is_some());
        board.get_by_label("Expand board toolbar").click();
        board.run_steps(2);
        assert!(board.query_by_label("Collapse board toolbar").is_some());
    }

    #[test]
    fn toolbar_selects_tools_and_can_be_collapsed() {
        let mut harness = harness();
        harness.get_by_label("Sticky note").click();
        harness.run_steps(2);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Sticky
        );
        click_scrolled_into_view(&mut harness, "Board minimap");
        assert!(harness.state().doc().unwrap().view.show_minimap);
        harness.get_by_label("Collapse board toolbar").click();
        harness.run_steps(2);
        assert!(harness.query_by_label("Sticky note").is_some());
        click_scrolled_into_view(&mut harness, "Select tool");
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Select
        );
        click_scrolled_into_view(&mut harness, "Sticky note");
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Sticky
        );
        harness.get_by_label("Expand board toolbar").click();
        harness.run_steps(2);
        assert!(harness.query_by_label("Sticky note").is_some());
    }

    #[test]
    fn shape_library_filters_and_selects_a_shape() {
        let mut harness = harness();
        let before = harness.state().doc().unwrap().doc.workbook().clone();
        let before_state = harness.state().doc().unwrap().doc.state_id();
        let before_dirty = harness.state().doc().unwrap().is_dirty();
        harness.get_by_label("Shape library").click();
        harness.run_steps(4);
        harness
            .get_by_role_and_label(egui::Role::TextInput, "Search shapes")
            .click();
        harness.run_steps(2);
        harness
            .get_by_role_and_label(egui::Role::TextInput, "Search shapes")
            .type_text("TRIANGLE");
        harness.run_steps(2);
        assert!(harness.query_by_label("Triangle").is_some());
        assert!(harness.query_by_label("Right triangle").is_some());
        assert!(harness.query_by_label("Pentagon").is_none());
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness
            .get_by_role_and_label(egui::Role::TextInput, "Search shapes")
            .type_text("Pentagon");
        harness.run_steps(2);
        assert!(harness.query_by_label("Triangle").is_none());
        harness
            .get_by_role_and_label(egui::Role::Button, "Pentagon")
            .click();
        harness.run_steps(2);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Shape(ShapeKind::Pentagon)
        );
        assert!(harness.query_by_label("Search shapes").is_none());

        harness.get_by_label("Shape library").click();
        harness.run_steps(4);
        assert_eq!(
            harness
                .get_by_role_and_label(egui::Role::Button, "Pentagon")
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        harness
            .get_by_role_and_label(egui::Role::TextInput, "Search shapes")
            .click();
        harness.run_steps(2);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness
            .get_by_role_and_label(egui::Role::TextInput, "Search shapes")
            .type_text("star");
        harness.run_steps(2);
        harness.get_by_label("Star").click();
        harness.run_steps(3);
        let doc = harness.state().doc().unwrap();
        assert_eq!(doc.view.board_tool, BoardTool::Shape(ShapeKind::Star));
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.doc.state_id(), before_state);
        assert_eq!(doc.is_dirty(), before_dirty);
    }

    #[test]
    fn armed_shape_hint_explains_placement_and_the_armed_tile_disarms() {
        let mut harness = harness();
        assert!(harness.query_by_label(ARMED_SHAPE_HINT).is_none());
        let arm = |harness: &mut Harness<'_, ThinodaApp>| {
            harness.get_by_label("Shape library").click();
            harness.run_steps(4);
            harness
                .get_by_role_and_label(egui::Role::Button, "Pentagon")
                .click();
            harness.run_steps(3);
        };
        arm(&mut harness);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Shape(ShapeKind::Pentagon)
        );
        assert!(harness.query_by_label(ARMED_SHAPE_HINT).is_some());

        arm(&mut harness);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Select
        );
        assert!(harness.query_by_label(ARMED_SHAPE_HINT).is_none());

        arm(&mut harness);
        harness.get_by_label("Dismiss").click();
        harness.run_steps(3);
        assert!(harness.query_by_label(ARMED_SHAPE_HINT).is_none());
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Shape(ShapeKind::Pentagon)
        );
        harness
            .state_mut()
            .doc_mut()
            .unwrap()
            .view
            .set_board_tool(BoardTool::Select);
        harness.run_steps(2);
        arm(&mut harness);
        assert!(harness.query_by_label(ARMED_SHAPE_HINT).is_none());
    }

    #[test]
    fn armed_shape_hint_is_hidden_on_narrow_canvases() {
        for (width, visible) in [(ARMED_SHAPE_HINT_MIN_WIDTH - 20.0, false), (700.0, true)] {
            let mut app = ThinodaApp::headless();
            app.doc_mut()
                .unwrap()
                .view
                .set_board_tool(BoardTool::Shape(ShapeKind::Ellipse));
            let mut harness = Harness::builder()
                .with_size(vec2(900.0, 600.0))
                .build_ui_state(
                    move |ui, app: &mut ThinodaApp| {
                        let canvas = Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 500.0));
                        app.armed_shape_hint(ui.ctx(), canvas);
                    },
                    app,
                );
            harness.run_steps(3);
            assert_eq!(
                harness.query_by_label(ARMED_SHAPE_HINT).is_some(),
                visible,
                "{width}"
            );
        }
    }

    #[test]
    fn shape_library_exposes_every_shape_as_a_named_grid_button() {
        let mut harness = Harness::builder()
            .with_size(vec2(800.0, 800.0))
            .build_ui_state(expanded_toolbar, ThinodaApp::headless());
        harness.run_steps(3);
        harness.get_by_label("Shape library").click();
        harness.run_steps(4);

        for shape in ShapeKind::ALL {
            let button = harness.get_by_role_and_label(egui::Role::Button, shape_name(shape));
            assert_eq!(
                button.accesskit_node().toggled(),
                Some(egui::accesskit::Toggled::False),
                "{shape:?}"
            );
        }
        let rectangle = harness.get_by_label("Rectangle").rect();
        let rounded = harness.get_by_label("Rounded rectangle").rect();
        let diamond = harness.get_by_label("Diamond").rect();
        assert_eq!(rectangle.size(), vec2(44.0, 44.0));
        assert_eq!(rectangle.top(), rounded.top());
        assert!(rounded.left() > rectangle.right());
        assert_eq!(rectangle.top(), diamond.top());

        harness.hover_at(harness.get_by_label("Rectangle").rect().center());
        harness.run_steps(90);
        assert!(
            harness.output().shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Rectangle")
            }),
            "shape name should appear as a tooltip"
        );
    }

    #[test]
    fn shape_library_uses_localized_search_accessible_names_and_tooltips() {
        fn german(key: &'static str) -> &'static str {
            i18n::t_in(i18n::Lang::De, key)
        }

        let mut harness = Harness::builder()
            .with_size(vec2(500.0, 500.0))
            .build_ui_state(
                |ui, selected: &mut ShapeKind| {
                    if let Some(shape) = shape_library(ui, ui.max_rect(), Some(*selected), german) {
                        *selected = shape;
                    }
                },
                ShapeKind::Rectangle,
            );
        harness.run_steps(3);
        let search = "Formen suchen";
        harness
            .get_by_role_and_label(egui::Role::TextInput, search)
            .click();
        harness.run_steps(2);
        harness.get_by_label(search).type_text("dReIeCk");
        harness.run_steps(3);
        assert!(harness.query_by_label("Dreieck").is_some());
        assert!(harness.query_by_label("Rechtwinkliges Dreieck").is_some());
        assert!(harness.query_by_label("Triangle").is_none());
        assert!(harness.query_by_label("Fünfeck").is_none());
        harness.hover_at(harness.get_by_label("Dreieck").rect().center());
        harness.run_steps(90);
        assert!(harness.output().shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Dreieck")
        }));

        harness.get_by_label(search).click();
        harness.run_steps(2);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness.get_by_label(search).type_text("FÜNF");
        harness.run_steps(3);
        assert!(harness.query_by_label("Dreieck").is_none());
        harness.get_by_label("Fünfeck").click();
        harness.run_steps(3);
        assert_eq!(*harness.state(), ShapeKind::Pentagon);
        assert_eq!(
            harness.get_by_label("Fünfeck").accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );

        harness.get_by_label(search).click();
        harness.run_steps(2);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness.get_by_label(search).type_text("no-such-shape");
        harness.run_steps(3);
        assert!(harness.query_by_label("Keine Treffer").is_some());
        assert!(harness.query_by_label("Fünfeck").is_none());
    }

    #[test]
    fn shape_library_supports_keyboard_and_accesskit_activation() {
        let mut app = ThinodaApp::headless();
        app.doc_mut().unwrap().view.board_tool = BoardTool::Shape(ShapeKind::Rectangle);
        let mut harness = Harness::builder()
            .with_size(vec2(800.0, 800.0))
            .build_ui_state(expanded_toolbar, app);
        harness.run_steps(3);
        let before = harness.state().doc().unwrap().doc.state_id();
        let trigger = harness.get_by_label("Shape library");
        assert_eq!(
            trigger.accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        assert_eq!(trigger.accesskit_node().data().is_expanded(), Some(false));
        assert_eq!(
            trigger.accesskit_node().has_popup(),
            Some(egui::accesskit::HasPopup::Menu)
        );
        trigger.focus();
        harness.run_steps(2);
        harness.key_press(egui::Key::Enter);
        harness.run_steps(4);
        assert!(harness.query_by_label("Search shapes").is_some());
        assert_eq!(
            harness
                .get_by_role_and_label(egui::Role::Button, "Shape library")
                .accesskit_node()
                .data()
                .is_expanded(),
            Some(true)
        );
        assert_eq!(
            harness.get_by_label("Rectangle").accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        assert_eq!(
            harness
                .get_by_label("Rounded rectangle")
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::False)
        );

        if !harness.get_by_label("Search shapes").is_focused() {
            harness.key_press(egui::Key::Tab);
            harness.run_steps(2);
        }
        assert!(harness.get_by_label("Search shapes").is_focused());
        harness.key_press(egui::Key::Tab);
        harness.run_steps(2);
        assert!(harness.get_by_label("Rectangle").is_focused());
        harness.key_press(egui::Key::ArrowRight);
        harness.run_steps(3);
        assert!(harness.get_by_label("Rounded rectangle").is_focused());
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(3);
        assert!(harness.get_by_label("Trapezoid").is_focused());
        harness.key_press(egui::Key::ArrowUp);
        harness.run_steps(3);
        assert!(harness.get_by_label("Rounded rectangle").is_focused());
        harness.key_press(egui::Key::ArrowLeft);
        harness.run_steps(3);
        assert!(harness.get_by_label("Rectangle").is_focused());
        harness.key_press(egui::Key::Tab);
        harness.run_steps(2);
        assert!(harness.get_by_label("Rounded rectangle").is_focused());
        harness.key_press(egui::Key::Space);
        harness.run_steps(3);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Shape(ShapeKind::RoundedRectangle)
        );
        assert!(harness.query_by_label("Search shapes").is_none());

        harness.get_by_label("Shape library").click();
        harness.run_steps(4);
        harness.get_by_label("Off-page connector").click_accesskit();
        harness.run_steps(3);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Shape(ShapeKind::OffPageConnector)
        );
        assert_eq!(harness.state().doc().unwrap().doc.state_id(), before);
        harness.get_by_label("Shape library").click();
        harness.run_steps(4);
        assert_eq!(
            harness
                .get_by_label("Off-page connector")
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        harness.get_by_label("Off-page connector").focus();
        harness.run_steps(12);
        let last = harness.get_by_label("Off-page connector").rect();
        let search = harness.get_by_label("Search shapes").rect();
        assert!(
            harness.ctx.content_rect().contains_rect(last) && last.top() >= search.bottom(),
            "focused tile is not visible: last={last:?}, search={search:?}"
        );
        assert!(harness.get_by_label("Off-page connector").is_focused());
        harness.key_press(egui::Key::Escape);
        harness.run_steps(3);
        assert!(harness.query_by_label("Search shapes").is_none());
        assert!(harness.get_by_label("Shape library").is_focused());
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Shape(ShapeKind::OffPageConnector)
        );
    }

    #[test]
    fn shape_library_keyboard_selection_does_not_edit_the_underlying_canvas() {
        let mut harness = harness();
        let before = harness.state().doc().unwrap().doc.workbook().clone();
        let state = harness.state().doc().unwrap().doc.state_id();
        harness.get_by_label("Shape library").focus();
        harness.run_steps(3);
        harness.key_press(egui::Key::Enter);
        harness.run_steps(4);
        harness.get_by_label("Triangle").focus();
        harness.run_steps(3);
        harness.key_press(egui::Key::Space);
        harness.run_steps(4);
        let doc = harness.state().doc().unwrap();
        assert_eq!(doc.view.board_tool, BoardTool::Shape(ShapeKind::Triangle));
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.doc.state_id(), state);
    }

    #[test]
    fn shape_tiles_draw_every_actual_outline_and_detail_in_both_themes() {
        for dark in [false, true] {
            let mut harness =
                Harness::builder()
                    .with_size(vec2(1400.0, 300.0))
                    .build_ui(move |ui| {
                        shape_test_theme(ui.ctx(), dark);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                            for shape in ShapeKind::ALL {
                                shape_tile(
                                    ui,
                                    shape,
                                    shape_name(shape),
                                    shape == ShapeKind::Star,
                                    44.0,
                                    false,
                                );
                            }
                        });
                    });
            harness.run_steps(3);
            let palette = thinoda_ui::Palette::of(&harness.ctx);
            assert_eq!(palette.dark, dark);
            for shape in ShapeKind::ALL {
                let rect = harness.get_by_label(shape_name(shape)).rect();
                let icon = Rect::from_center_size(rect.center(), vec2(44.0 * 0.62, 44.0 * 0.52));
                let to_points = |outline: Vec<thinoda_core::Vec2>| {
                    outline
                        .into_iter()
                        .map(|p| icon.min + vec2(p.x * icon.width(), p.y * icon.height()))
                        .collect::<Vec<_>>()
                };
                let outline = to_points(shape.unit_outline());
                let color = if shape == ShapeKind::Star {
                    palette.accent
                } else {
                    palette.text
                };
                let find_path = |points: &[egui::Pos2], closed| {
                    harness.output().shapes.iter().any(|clipped| {
                        matches!(&clipped.shape, egui::Shape::Path(path)
                            if path.closed == closed
                                && path.points == points
                                && path.fill == Color32::TRANSPARENT
                                && path.stroke.color == egui::epaint::ColorMode::Solid(color))
                    })
                };
                assert!(
                    find_path(&outline, true),
                    "missing {shape:?} in dark={dark}"
                );
                for detail in shape.unit_details() {
                    assert!(
                        find_path(&to_points(detail), false),
                        "missing {shape:?} detail in dark={dark}"
                    );
                }
                assert!(
                    harness.output().shapes.iter().any(|clipped| {
                        matches!(&clipped.shape, egui::Shape::Rect(tile)
                        if tile.rect == rect.shrink(0.5)
                            && tile.corner_radius.nw > 0
                            && tile.fill == if shape == ShapeKind::Star {
                                palette.accent_soft
                            } else {
                                palette.surface_alt
                            })
                    }),
                    "missing rounded tile for {shape:?} in dark={dark}"
                );
            }
        }
    }

    #[test]
    fn shape_library_clamps_and_scrolls_in_short_narrow_windows() {
        fn painted_rectangles(shape: &egui::Shape, rectangles: &mut Vec<(Rect, Color32)>) {
            match shape {
                egui::Shape::Rect(rect) => rectangles.push((rect.rect, rect.fill)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        painted_rectangles(shape, rectangles);
                    }
                }
                _ => {}
            }
        }

        for dark in [false, true] {
            for size in [
                vec2(320.0, 240.0),
                vec2(180.0, 120.0),
                vec2(106.0, 80.0),
                vec2(520.0, 180.0),
            ] {
                for collapsed in [false, true] {
                    let mut app = ThinodaApp::headless();
                    app.doc_mut().unwrap().view.dark = dark;
                    let mut harness = Harness::builder().with_size(size).build_ui_state(
                        move |ui, app: &mut ThinodaApp| {
                            shape_test_theme(ui.ctx(), dark);
                            expanded_toolbar(ui, app);
                        },
                        app,
                    );
                    harness.run_steps(4);
                    if collapsed {
                        harness.get_by_label("Collapse board toolbar").click();
                        harness.run_steps(3);
                    }
                    click_scrolled_into_view(&mut harness, "Shape library");
                    harness.run_steps(20);
                    let bounds = harness.ctx.content_rect();
                    assert!(
                        harness
                            .query_by_role_and_label(egui::Role::Menu, "Shape library")
                            .is_some()
                    );
                    let search = harness.get_by_label("Search shapes").rect();
                    let first = harness.get_by_label("Rectangle").rect();
                    let trigger = harness
                        .get_by_role_and_label(egui::Role::Button, "Shape library")
                        .rect();
                    let mut rectangles = Vec::new();
                    for clipped in &harness.output().shapes {
                        painted_rectangles(&clipped.shape, &mut rectangles);
                    }
                    let popup = rectangles
                        .iter()
                        .filter(|(rect, fill)| {
                            *fill == thinoda_ui::Palette::of(&harness.ctx).surface
                                && rect.contains_rect(search)
                                && rect.contains_rect(first)
                        })
                        .map(|(rect, _)| *rect)
                        .min_by(|a, b| {
                            (a.width() * a.height()).total_cmp(&(b.width() * b.height()))
                        })
                        .unwrap_or_else(|| {
                            panic!(
                                "popup frame enclosing search and first tile not found: \
                                 dark={dark} size={size:?} collapsed={collapsed} search={search:?} \
                                 first={first:?} surface={:?}, enclosing_search={:?}",
                                thinoda_ui::Palette::of(&harness.ctx).surface,
                                rectangles
                                    .iter()
                                    .filter(|(rect, _)| rect.contains_rect(search))
                                    .collect::<Vec<_>>(),
                            )
                        });
                    assert!(
                        bounds.contains_rect(popup)
                            && bounds.contains_rect(search)
                            && bounds.contains_rect(first),
                        "picker outside viewport: dark={dark} size={size:?} collapsed={collapsed}, \
                         bounds={bounds:?} popup={popup:?} search={search:?} first={first:?} trigger={trigger:?}"
                    );
                    harness.get_by_label("Off-page connector").scroll_to_me();
                    harness.run_steps(12);
                    let last = harness.get_by_label("Off-page connector").rect();
                    assert!(
                        bounds.contains_rect(last),
                        "last tile outside viewport: dark={dark} size={size:?} \
                         collapsed={collapsed}, bounds={bounds:?} last={last:?}"
                    );
                    harness.get_by_label("Off-page connector").click();
                    harness.run_steps(3);
                    assert_eq!(
                        harness.state().doc().unwrap().view.board_tool,
                        BoardTool::Shape(ShapeKind::OffPageConnector),
                        "last tile not clickable: dark={dark} size={size:?} collapsed={collapsed}"
                    );
                }
            }
        }
    }

    #[test]
    fn shape_library_scrollbar_does_not_cover_tile_hit_targets() {
        for size in [vec2(320.0, 240.0), vec2(180.0, 120.0)] {
            let mut harness = Harness::builder()
                .with_size(size)
                .build_ui_state(expanded_toolbar, ThinodaApp::headless());
            harness.run_steps(4);
            click_scrolled_into_view(&mut harness, "Shape library");
            harness.run_steps(8);
            let shape = ShapeKind::ALL
                .into_iter()
                .max_by(|a, b| {
                    harness
                        .get_by_label(shape_name(*a))
                        .rect()
                        .right()
                        .total_cmp(&harness.get_by_label(shape_name(*b)).rect().right())
                })
                .unwrap();
            harness.get_by_label(shape_name(shape)).scroll_to_me();
            harness.run_steps(12);
            let rect = harness.get_by_label(shape_name(shape)).rect();
            assert!(harness.ctx.content_rect().contains_rect(rect), "{rect:?}");
            click_point(
                &mut harness,
                egui::pos2(rect.right() - 1.0, rect.center().y),
            );
            assert_eq!(
                harness.state().doc().unwrap().view.board_tool,
                BoardTool::Shape(shape),
                "right edge of {shape:?} was not clickable at {size:?}"
            );
        }
    }

    #[test]
    fn shape_library_reflows_and_resets_scrolling_when_search_changes() {
        let mut harness = Harness::builder()
            .with_size(vec2(800.0, 800.0))
            .build_ui_state(expanded_toolbar, ThinodaApp::headless());
        harness.run_steps(4);
        let before = harness.state().doc().unwrap().doc.state_id();
        harness.get_by_label("Shape library").click();
        harness.run_steps(5);
        let wide_first = harness.get_by_label("Rectangle").rect();
        let wide_sixth = harness.get_by_label("Right triangle").rect();
        assert_eq!(wide_first.top(), wide_sixth.top());

        harness.set_size(vec2(180.0, 160.0));
        harness.run_steps(8);
        let bounds = harness.ctx.content_rect();
        let search = harness.get_by_label("Search shapes").rect();
        let first = harness.get_by_label("Rectangle").rect();
        let sixth = harness.get_by_label("Right triangle").rect();
        assert!(
            bounds.contains_rect(search) && bounds.contains_rect(first),
            "search={search:?} first={first:?} bounds={bounds:?}"
        );
        assert!(
            sixth.top() > first.top(),
            "grid should have fewer columns: first={first:?}, sixth={sixth:?}, bounds={bounds:?}"
        );
        let below_first = ShapeKind::ALL
            .into_iter()
            .find(|shape| {
                let rect = harness.get_by_label(shape_name(*shape)).rect();
                rect.top() > first.top() && (rect.left() - first.left()).abs() < 1.0
            })
            .unwrap();
        harness.get_by_label("Rectangle").focus();
        harness.run_steps(3);
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(3);
        assert!(harness.get_by_label(shape_name(below_first)).is_focused());
        let last_before = harness.get_by_label("Off-page connector").rect();
        harness.hover_at(first.center());
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -1200.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(15);
        let last_after = harness.get_by_label("Off-page connector").rect();
        assert!(
            last_after.top() < last_before.top() - 10.0,
            "wheel did not scroll grid: before={last_before:?}, after={last_after:?}"
        );

        harness.get_by_label("Search shapes").click();
        harness.run_steps(2);
        harness.get_by_label("Search shapes").type_text("rectangle");
        harness.run_steps(5);
        let first = harness.get_by_label("Rectangle").rect();
        assert!(bounds.contains_rect(first), "filtered result={first:?}");
        assert!(harness.query_by_label("Off-page connector").is_none());
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness.key_press(egui::Key::Backspace);
        harness.run_steps(5);
        assert!(bounds.contains_rect(harness.get_by_label("Rectangle").rect()));
        assert!(harness.query_by_label("Off-page connector").is_some());

        harness.set_size(vec2(800.0, 800.0));
        harness.run_steps(8);
        assert_eq!(
            harness.get_by_label("Rectangle").rect().top(),
            harness.get_by_label("Ellipse").rect().top()
        );
        assert_eq!(harness.state().doc().unwrap().doc.state_id(), before);
    }

    #[test]
    fn shape_library_accepts_touch_taps_but_not_canceled_taps_or_swipes() {
        fn touch(
            harness: &mut Harness<'_, ThinodaApp>,
            phase: egui::TouchPhase,
            pos: egui::Pos2,
            pointer_pressed: Option<bool>,
        ) {
            harness.event(egui::Event::PointerMoved(pos));
            harness.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(1),
                phase,
                pos,
                force: None,
            });
            if let Some(pressed) = pointer_pressed {
                harness.event(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            harness.run_steps(1);
        }

        let mut harness = Harness::builder()
            .with_size(vec2(320.0, 240.0))
            .build_ui_state(expanded_toolbar, ThinodaApp::headless());
        harness.run_steps(4);
        let before = harness.state().doc().unwrap().doc.workbook().clone();
        let before_state = harness.state().doc().unwrap().doc.state_id();
        click_scrolled_into_view(&mut harness, "Shape library");
        harness.run_steps(4);

        let first = harness.get_by_label("Rectangle").rect().center();
        touch(&mut harness, egui::TouchPhase::Start, first, Some(true));
        touch(&mut harness, egui::TouchPhase::Cancel, first, Some(false));
        harness.run_steps(4);
        assert!(harness.query_by_label("Search shapes").is_some());
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Select
        );

        touch(&mut harness, egui::TouchPhase::Start, first, Some(true));
        let second = first + vec2(8.0, 0.0);
        for phase in [egui::TouchPhase::Start, egui::TouchPhase::End] {
            harness.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(2),
                phase,
                pos: second,
                force: None,
            });
            harness.run_steps(1);
        }
        touch(&mut harness, egui::TouchPhase::End, first, Some(false));
        harness.run_steps(4);
        assert!(harness.query_by_label("Search shapes").is_some());
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Select
        );

        let start = harness.get_by_label("Triangle").rect().center();
        let last_before = harness.get_by_label("Off-page connector").rect();
        touch(&mut harness, egui::TouchPhase::Start, start, Some(true));
        for dy in [15.0, 45.0, 75.0] {
            touch(
                &mut harness,
                egui::TouchPhase::Move,
                start - vec2(0.0, dy),
                None,
            );
        }
        touch(
            &mut harness,
            egui::TouchPhase::End,
            start - vec2(0.0, 75.0),
            Some(false),
        );
        harness.run_steps(8);
        let last_after = harness.get_by_label("Off-page connector").rect();
        assert!(
            last_after.top() < last_before.top() - 10.0,
            "touch swipe did not scroll grid: before={last_before:?}, after={last_after:?}"
        );
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Select
        );

        harness.get_by_label("Off-page connector").scroll_to_me();
        harness.run_steps(12);
        let last = harness.get_by_label("Off-page connector").rect();
        assert!(harness.ctx.content_rect().contains_rect(last), "{last:?}");
        touch(
            &mut harness,
            egui::TouchPhase::Start,
            last.center(),
            Some(true),
        );
        touch(
            &mut harness,
            egui::TouchPhase::End,
            last.center(),
            Some(false),
        );
        harness.run_steps(4);
        let doc = harness.state().doc().unwrap();
        assert_eq!(
            doc.view.board_tool,
            BoardTool::Shape(ShapeKind::OffPageConnector)
        );
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.doc.state_id(), before_state);
    }

    #[cfg(feature = "gpu-tests")]
    #[test]
    fn shape_library_visual_previews() {
        let Some(directory) = std::env::var_os("THINODA_BOARD_VISUAL_DIR") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).expect("create shape library preview directory");
        for dark in [false, true] {
            for (size, width) in [
                (vec2(800.0, 600.0), "wide"),
                (vec2(320.0, 240.0), "compact"),
            ] {
                let mut app = ThinodaApp::headless();
                app.doc_mut().unwrap().view.dark = dark;
                app.doc_mut().unwrap().view.board_tool = BoardTool::Shape(ShapeKind::Star);
                let mut harness = Harness::builder().wgpu().with_size(size).build_ui_state(
                    move |ui, app: &mut ThinodaApp| {
                        shape_test_theme(ui.ctx(), dark);
                        expanded_toolbar(ui, app);
                    },
                    app,
                );
                harness.run_steps(4);
                click_scrolled_into_view(&mut harness, "Shape library");
                harness.run_steps(6);
                let theme = if dark { "dark" } else { "light" };
                harness
                    .render()
                    .expect("shape library render")
                    .save(directory.join(format!("shape-library-{theme}-{width}.png")))
                    .unwrap();
            }
        }
    }

    #[cfg(feature = "gpu-tests")]
    #[test]
    fn ink_context_visual_previews() {
        let Some(directory) = std::env::var_os("THINODA_BOARD_VISUAL_DIR") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).expect("create ink context preview directory");
        for dark in [false, true] {
            for (size, width) in [
                (vec2(800.0, 600.0), "wide"),
                (vec2(320.0, 240.0), "compact"),
            ] {
                let mut app = ThinodaApp::headless();
                app.doc_mut().unwrap().view.dark = dark;
                app.doc_mut().unwrap().view.board_tool = BoardTool::Pen;
                let mut harness = Harness::builder().wgpu().with_size(size).build_ui_state(
                    move |ui, app: &mut ThinodaApp| {
                        shape_test_theme(ui.ctx(), dark);
                        expanded_toolbar(ui, app);
                    },
                    app,
                );
                harness.run_steps(4);
                let theme = if dark { "dark" } else { "light" };
                for (control, name) in [("Ink color", "color"), ("Ink width", "width")] {
                    click_scrolled_into_view(&mut harness, control);
                    harness.run_steps(6);
                    harness
                        .render()
                        .expect("ink context render")
                        .save(directory.join(format!("ink-{name}-{theme}-{width}.png")))
                        .unwrap();
                    harness.key_press(egui::Key::Escape);
                    harness.run_steps(3);
                }
            }
        }
    }

    #[test]
    fn toolbar_exposes_ink_tools_and_selection_free_image_cards() {
        let mut harness = harness();
        harness
            .state_mut()
            .doc_mut()
            .unwrap()
            .view
            .selection
            .clear();
        harness.run_steps(2);
        assert!(harness.state().command_available(&Cmd::BoardImage));
        assert!(harness.query_by_label("Image card").is_some());

        harness.get_by_label("Pen").click();
        harness.run_steps(2);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Pen
        );
        assert!(harness.query_by_label("Ink width").is_some());
        assert!(harness.query_by_label("Ink color").is_some());
        assert!(harness.query_by_label("Stroke type").is_some());

        click_scrolled_into_view(&mut harness, "Pixel eraser");
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::PixelEraser
        );
        assert!(harness.query_by_label("Eraser radius").is_some());
        assert!(harness.query_by_label("Lasso").is_some());
    }

    #[test]
    fn automatic_ink_color_tracks_canvas_background_without_overwriting_custom_color() {
        let mut app = ThinodaApp::headless();
        app.doc_mut().unwrap().view.board_tool = BoardTool::Pen;
        app.doc_mut().unwrap().view.dark = false;
        let mut harness = Harness::builder()
            .with_size(vec2(800.0, 800.0))
            .build_ui_state(expanded_toolbar, app);
        harness.run_steps(3);
        assert_eq!(
            harness.state().doc().unwrap().view.ink_settings.color,
            Color::BLACK
        );
        harness.state_mut().doc_mut().unwrap().view.dark = true;
        harness.run_steps(2);
        assert_eq!(
            harness.state().doc().unwrap().view.ink_settings.color,
            Color::WHITE
        );

        harness.get_by_label("Ink color").click();
        harness.run_steps(2);
        harness.get_by_label("Auto").click();
        harness.run_steps(2);
        let custom = Color::rgb(10, 200, 70);
        let view = &mut harness.state_mut().doc_mut().unwrap().view;
        view.ink_settings.color = custom;
        view.dark = false;
        harness.run_steps(2);
        assert_eq!(
            harness.state().doc().unwrap().view.ink_settings.color,
            custom
        );
    }

    #[test]
    fn collapsed_toolbox_keeps_accessible_icons_without_painting_labels() {
        let mut harness = Harness::builder()
            .with_size(vec2(800.0, 800.0))
            .build_ui_state(expanded_toolbar, ThinodaApp::headless());
        harness.run_steps(3);
        let paints_label = |h: &Harness<'_, ThinodaApp>, label: &str| {
            h.output().shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == label)
            })
        };
        assert!(paints_label(&harness, "Sticky note"));
        harness.get_by_label("Collapse board toolbar").click();
        harness.run_steps(3);
        for label in ["Select tool", "Hand tool", "Sticky note", "Text card"] {
            assert!(harness.query_by_label(label).is_some(), "{label}");
            assert!(!paints_label(&harness, label), "{label}");
        }
        harness.hover_at(harness.get_by_label("Sticky note").rect().center());
        harness.run_steps(90);
        assert!(paints_label(&harness, "Sticky note"), "icon tooltip");
        harness.event(egui::Event::PointerGone);
        harness.run_steps(3);
        harness.get_by_label("Sticky note").click();
        harness.run_steps(2);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Sticky
        );
    }

    #[test]
    fn ink_context_controls_and_stroke_type_work_in_both_toolbox_states() {
        for collapsed in [false, true] {
            let mut app = ThinodaApp::headless();
            app.doc_mut().unwrap().view.board_tool = BoardTool::Pen;
            let mut harness = Harness::builder()
                .with_size(vec2(800.0, 1000.0))
                .build_ui_state(expanded_toolbar, app);
            harness.run_steps(3);
            if collapsed {
                harness.get_by_label("Collapse board toolbar").click();
                harness.run_steps(2);
            }
            for label in ["Ink color", "Ink width", "Stroke type", "Opacity"] {
                assert!(harness.query_by_label(label).is_some(), "{label}");
            }
            harness.get_by_label("Ink width").click();
            harness.run_steps(2);
            harness
                .get_by_role_and_label(egui::Role::SpinButton, "Ink width")
                .click();
            harness.run_steps(2);
            harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
            harness
                .get_by_role_and_label(egui::Role::SpinButton, "Ink width")
                .type_text("7.5");
            harness.key_press(egui::Key::Enter);
            harness.run_steps(3);
            assert_eq!(harness.state().doc().unwrap().view.ink_settings.width, 7.5);
            harness.key_press(egui::Key::Escape);
            harness.run_steps(2);
            harness.get_by_label("Stroke type").click();
            harness.run_steps(2);
            harness
                .query_all_by_label("Highlighter")
                .min_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y))
                .unwrap()
                .click();
            harness.run_steps(3);
            assert_eq!(
                harness.state().doc().unwrap().view.board_tool,
                BoardTool::Highlighter
            );
            harness.get_by_label("Ink color").click();
            harness.run_steps(2);
            harness.get_by_label("Auto").click();
            harness.run_steps(2);
            assert!(harness.query_all_by_label("Ink color").count() >= 2);
            harness.key_press(egui::Key::Escape);
            harness.run_steps(2);
            assert_eq!(
                harness.state().doc().unwrap().view.board_tool,
                BoardTool::Highlighter
            );
        }
    }

    #[test]
    fn ink_context_palette_and_width_presets_are_keyboard_accessible() {
        for dark in [false, true] {
            for collapsed in [false, true] {
                let mut app = ThinodaApp::headless();
                let doc = app.doc_mut().unwrap();
                doc.view.board_tool = BoardTool::Pen;
                doc.view.dark = dark;
                doc.view.ink_settings.opacity = 0.4;
                let before = doc.doc.workbook().clone();
                let state = doc.doc.state_id();
                let mut harness = Harness::builder()
                    .with_size(vec2(800.0, 800.0))
                    .build_ui_state(
                        move |ui, app: &mut ThinodaApp| {
                            shape_test_theme(ui.ctx(), dark);
                            expanded_toolbar(ui, app);
                        },
                        app,
                    );
                harness.run_steps(3);
                if collapsed {
                    harness.get_by_label("Collapse board toolbar").click();
                    harness.run_steps(3);
                }
                harness
                    .get_by_role_and_label(egui::Role::Button, "Ink color")
                    .click();
                harness.run_steps(3);
                let blue = harness.get_by_role_and_label(egui::Role::ColorWell, "#2E7CF6");
                blue.focus();
                harness.run_steps(2);
                harness.key_press(egui::Key::Space);
                harness.run_steps(3);
                let doc = harness.state().doc().unwrap();
                assert_eq!(doc.view.ink_settings.color, Color::rgb(0x2e, 0x7c, 0xf6));
                assert_eq!(doc.view.ink_settings.opacity, 0.4);
                assert_eq!(doc.doc.workbook(), &before);
                assert_eq!(doc.doc.state_id(), state);
                assert_eq!(
                    harness
                        .get_by_role_and_label(egui::Role::ColorWell, "#2E7CF6")
                        .accesskit_node()
                        .toggled(),
                    Some(egui::accesskit::Toggled::True)
                );
                harness.key_press(egui::Key::Escape);
                harness.run_steps(2);
                harness
                    .get_by_role_and_label(egui::Role::Button, "Ink width")
                    .click();
                harness.run_steps(3);
                let wide = harness.get_by_role_and_label(egui::Role::Button, "Ink width: 8");
                wide.focus();
                harness.run_steps(2);
                harness.key_press(egui::Key::Enter);
                harness.run_steps(3);
                let doc = harness.state().doc().unwrap();
                assert_eq!(doc.view.ink_settings.width, 8.0);
                assert_eq!(doc.doc.workbook(), &before);
                assert_eq!(doc.doc.state_id(), state);
                assert_eq!(
                    harness
                        .get_by_role_and_label(egui::Role::Button, "Ink width: 8")
                        .accesskit_node()
                        .toggled(),
                    Some(egui::accesskit::Toggled::True)
                );
            }
        }
    }

    #[test]
    fn ink_palette_preserves_alpha_and_can_return_to_automatic_color() {
        let mut app = ThinodaApp::headless();
        let view = &mut app.doc_mut().unwrap().view;
        view.board_tool = BoardTool::Pen;
        view.ink_settings.color = Color::rgba(100, 120, 140, 96);
        view.ink_settings.opacity = 0.6;
        let mut harness = Harness::builder()
            .with_size(vec2(800.0, 800.0))
            .build_ui_state(expanded_toolbar, app);
        harness.run_steps(3);
        harness
            .get_by_role_and_label(egui::Role::Button, "Ink color")
            .click();
        harness.run_steps(3);
        harness
            .get_by_role_and_label(egui::Role::ColorWell, "#2E7CF6")
            .click();
        harness.run_steps(3);
        let doc = harness.state().doc().unwrap();
        assert_eq!(
            doc.view.ink_settings.color,
            Color::rgba(0x2e, 0x7c, 0xf6, 96)
        );
        assert_eq!(doc.view.ink_settings.opacity, 0.6);
        assert_eq!(
            harness
                .get_by_role_and_label(egui::Role::ColorWell, "#2E7CF6")
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        harness.get_by_label("Auto").click();
        harness.run_steps(3);
        let doc = harness.state().doc().unwrap();
        let sheet = doc.doc.sheet();
        let background =
            thinoda_core::theme::Theme::for_settings(&sheet.theme).background(&sheet.theme, false);
        assert_eq!(
            doc.view.ink_settings.color,
            Color::highest_contrast_on(background)
        );
    }

    #[test]
    fn ink_palette_fits_a_tall_window_without_clipping_swatches() {
        let mut app = ThinodaApp::headless();
        app.doc_mut().unwrap().view.board_tool = BoardTool::Pen;
        let mut harness = Harness::builder()
            .with_size(vec2(800.0, 600.0))
            .build_ui_state(expanded_toolbar, app);
        harness.run_steps(4);
        harness.get_by_label("Ink color").click();
        harness.run_steps(12);
        for color in crate::ui::SWATCHES {
            let label = format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b());
            let rect = harness
                .get_by_role_and_label(egui::Role::ColorWell, &label)
                .rect()
                .shrink(2.0);
            assert!(
                harness.output().shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(painted)
                        if painted.rect == rect
                            && painted.fill == color
                            && shape.clip_rect.contains_rect(rect))
                }),
                "swatch {label} is clipped despite the available space"
            );
        }
    }

    #[test]
    fn ink_context_palettes_scroll_in_short_narrow_windows() {
        for dark in [false, true] {
            for collapsed in [false, true] {
                for size in [vec2(320.0, 240.0), vec2(180.0, 120.0)] {
                    let mut app = ThinodaApp::headless();
                    app.doc_mut().unwrap().view.board_tool = BoardTool::Pen;
                    app.doc_mut().unwrap().view.dark = dark;
                    let mut harness = Harness::builder().with_size(size).build_ui_state(
                        move |ui, app: &mut ThinodaApp| {
                            shape_test_theme(ui.ctx(), dark);
                            expanded_toolbar(ui, app);
                        },
                        app,
                    );
                    harness.run_steps(4);
                    if collapsed {
                        harness.get_by_label("Collapse board toolbar").click();
                        harness.run_steps(3);
                    }
                    click_scrolled_into_view(&mut harness, "Ink color");
                    harness
                        .get_by_role_and_label(egui::Role::ColorWell, "#FFFFFF")
                        .scroll_to_me();
                    harness.run_steps(12);
                    let color = harness.get_by_role_and_label(egui::Role::ColorWell, "#FFFFFF");
                    assert!(
                        harness.ctx.content_rect().contains_rect(color.rect()),
                        "palette color outside viewport: dark={dark} collapsed={collapsed} size={size:?}"
                    );
                    color.click();
                    harness.run_steps(3);
                    assert_eq!(
                        harness.state().doc().unwrap().view.ink_settings.color,
                        Color::WHITE,
                        "palette color not activated: dark={dark} collapsed={collapsed} size={size:?}"
                    );
                    let target = crate::ui::SWATCHES
                        .iter()
                        .copied()
                        .max_by(|a, b| {
                            let label =
                                |c: &Color32| format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
                            harness
                                .get_by_role_and_label(egui::Role::ColorWell, &label(a))
                                .rect()
                                .right()
                                .total_cmp(
                                    &harness
                                        .get_by_role_and_label(egui::Role::ColorWell, &label(b))
                                        .rect()
                                        .right(),
                                )
                        })
                        .unwrap();
                    let label = format!("#{:02X}{:02X}{:02X}", target.r(), target.g(), target.b());
                    harness
                        .get_by_role_and_label(egui::Role::ColorWell, &label)
                        .scroll_to_me();
                    harness.run_steps(12);
                    let rect = harness
                        .get_by_role_and_label(egui::Role::ColorWell, &label)
                        .rect();
                    assert!(harness.ctx.content_rect().contains_rect(rect), "{rect:?}");
                    let other = crate::ui::SWATCHES
                        .iter()
                        .find(|color| **color != target)
                        .unwrap();
                    harness
                        .state_mut()
                        .doc_mut()
                        .unwrap()
                        .view
                        .ink_settings
                        .color = Color::rgb(other.r(), other.g(), other.b());
                    click_point(
                        &mut harness,
                        egui::pos2(rect.right() - 1.0, rect.center().y),
                    );
                    assert_eq!(
                        harness.state().doc().unwrap().view.ink_settings.color,
                        Color::rgb(target.r(), target.g(), target.b()),
                        "palette color edge not activated: dark={dark} collapsed={collapsed} size={size:?}"
                    );
                    harness.key_press(egui::Key::Escape);
                    harness.run_steps(3);
                    click_scrolled_into_view(&mut harness, "Ink width");
                    harness
                        .get_by_role_and_label(egui::Role::Button, "Ink width: 16")
                        .scroll_to_me();
                    harness.run_steps(12);
                    let width = harness.get_by_role_and_label(egui::Role::Button, "Ink width: 16");
                    assert!(
                        harness.ctx.content_rect().contains_rect(width.rect()),
                        "width preset outside viewport: dark={dark} collapsed={collapsed} size={size:?}"
                    );
                    width.click();
                    harness.run_steps(3);
                    assert_eq!(harness.state().doc().unwrap().view.ink_settings.width, 16.0);
                }
            }
        }
    }

    #[test]
    fn collapsed_tools_support_keyboard_focus_and_activation() {
        let mut harness = Harness::builder()
            .with_size(vec2(800.0, 800.0))
            .build_ui_state(expanded_toolbar, ThinodaApp::headless());
        harness.run_steps(3);
        harness.get_by_label("Collapse board toolbar").focus();
        harness.run_steps(2);
        harness.key_press(egui::Key::Space);
        harness.run_steps(3);
        harness.get_by_label("Select tool").focus();
        harness.run_steps(2);
        harness.key_press(egui::Key::Tab);
        harness.run_steps(2);
        assert!(harness.get_by_label("Hand tool").is_focused());
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Hand
        );
    }

    #[test]
    fn toolbox_uses_available_height_before_scrolling() {
        let mut harness = Harness::builder()
            .with_size(vec2(800.0, 800.0))
            .build_ui_state(expanded_toolbar, ThinodaApp::headless());
        harness.run_steps(4);
        let last = harness.get_by_label("Board minimap").rect();
        assert!(
            harness.ctx.content_rect().contains_rect(last),
            "last tool should fit without scrolling: {last:?}"
        );
        harness.get_by_label("Board minimap").click();
        harness.run_steps(2);
        assert!(harness.state().doc().unwrap().view.show_minimap);
    }

    #[test]
    fn toolbox_starts_near_top_when_no_toolbar_blocks_it() {
        let mut harness = Harness::builder()
            .with_size(vec2(800.0, 500.0))
            .build_ui_state(expanded_toolbar, ThinodaApp::headless());
        harness.run_steps(3);
        let toolbar = harness
            .ctx
            .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_board_toolbar")))
            .unwrap();
        assert!(
            toolbar.top() - harness.ctx.content_rect().top() <= 16.0,
            "unused space above toolbox: {toolbar:?}"
        );
    }

    #[test]
    fn toolbox_avoids_editing_toolbar_in_a_narrow_window() {
        let mut app = ThinodaApp::headless();
        app.new_doc(thinoda_core::templates::instantiate("kanban-board"));
        let mut harness = Harness::builder()
            .with_size(vec2(700.0, 500.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(5);
        harness.get_by_label("Expand board toolbar").click();
        harness.run_steps(3);
        let (board, editing) = harness.ctx.memory(|memory| {
            (
                memory.area_rect(egui::Id::unique("thinoda_board_toolbar")),
                memory.area_rect(egui::Id::unique("thinoda_editing_toolbar")),
            )
        });
        let (board, editing) = (board.unwrap(), editing.unwrap());
        assert!(
            !board.intersects(editing),
            "board and editing toolbars must not overlap: board={board:?}, editing={editing:?}"
        );
        harness.get_by_label("Sticky note").click();
        harness.run_steps(3);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Sticky,
            "board tools remain clickable in a narrow window"
        );
    }

    #[test]
    fn selected_card_toolbar_does_not_cover_board_tools() {
        for collapsed in [false, true] {
            let mut app = ThinodaApp::headless();
            app.new_doc(thinoda_core::templates::instantiate("kanban-board"));
            let mut harness = Harness::builder()
                .with_size(vec2(1200.0, 900.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            harness.run_steps(5);
            if !collapsed {
                harness.get_by_label("Expand board toolbar").click();
                harness.run_steps(3);
            }
            let (board, topic) = harness.ctx.memory(|memory| {
                (
                    memory.area_rect(egui::Id::unique("thinoda_board_toolbar")),
                    memory.area_rect(egui::Id::unique("thinoda_topic_toolbar")),
                )
            });
            let (board, topic) = (board.unwrap(), topic.unwrap());
            assert!(
                !board.intersects(topic),
                "toolbars overlap when collapsed={collapsed}: board={board:?}, topic={topic:?}"
            );
            harness.get_by_label("Sticky note").click();
            harness.run_steps(3);
            assert_eq!(
                harness.state().doc().unwrap().view.board_tool,
                BoardTool::Sticky,
                "the selected-card toolbar must not intercept board-tool clicks"
            );
        }
    }

    #[test]
    fn narrow_toolbar_keeps_a_fitting_topic_toolbar_available() {
        let mut app = ThinodaApp::headless();
        app.settings.ai.enabled = false;
        app.new_doc(thinoda_core::templates::instantiate("kanban-board"));
        let mut harness = Harness::builder()
            .with_size(vec2(700.0, 500.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(5);
        let topic_id = egui::Id::unique("thinoda_topic_toolbar");
        let (board, topic, visible) = harness.ctx.memory(|memory| {
            (
                memory.area_rect(egui::Id::unique("thinoda_board_toolbar")),
                memory.area_rect(topic_id),
                memory
                    .areas()
                    .visible_layer_ids()
                    .iter()
                    .any(|layer| layer.id == topic_id),
            )
        });
        assert!(
            visible,
            "a fitting selected-topic toolbar must remain available"
        );
        assert!(!board.unwrap().intersects(topic.unwrap()));
    }

    #[test]
    fn selected_card_toolbar_keeps_board_frame_titles_visible() {
        for collapsed in [false, true] {
            let mut app = ThinodaApp::headless();
            app.new_doc(thinoda_core::templates::instantiate("kanban-board"));
            let mut harness = Harness::builder()
                .with_size(vec2(1200.0, 900.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            harness.run_steps(5);
            if !collapsed {
                harness.get_by_label("Expand board toolbar").click();
                harness.run_steps(3);
            }

            let topic_id = egui::Id::unique("thinoda_topic_toolbar");
            let (toolbar, visible) = harness.ctx.memory(|memory| {
                (
                    memory.area_rect(topic_id),
                    memory
                        .areas()
                        .visible_layer_ids()
                        .iter()
                        .any(|layer| layer.id == topic_id),
                )
            });
            assert!(visible, "the selected-card toolbar should remain available");
            let toolbar = toolbar.unwrap();
            let doc = harness.state().doc().unwrap();
            let origin = doc.view.canvas_rect().center() + doc.view.offset;
            let zoom = doc.view.zoom;
            for zone in &doc.doc.sheet().zones {
                let min = origin + vec2(zone.frame.min.x, zone.frame.min.y) * zoom;
                let max = origin + vec2(zone.frame.max().x, zone.frame.max().y) * zoom;
                let title = Rect::from_min_max(min, egui::pos2(max.x, (min.y + 24.0).min(max.y)));
                assert!(
                    !toolbar.intersects(title),
                    "selected-card toolbar obscures the {} frame title when collapsed={collapsed}: {toolbar:?}, {title:?}",
                    zone.name
                );
            }
        }
    }

    fn visible_topic_toolbar(harness: &Harness<'_, ThinodaApp>) -> Option<Rect> {
        let id = egui::Id::unique("thinoda_topic_toolbar");
        harness.ctx.memory(|memory| {
            memory
                .areas()
                .visible_layer_ids()
                .iter()
                .any(|layer| layer.id == id)
                .then(|| memory.area_rect(id))
                .flatten()
        })
    }

    #[test]
    fn affinity_toolbar_never_paints_over_cards_during_initial_layout() {
        let mut app = ThinodaApp::headless();
        app.settings.ai.enabled = false;
        app.new_doc(thinoda_core::templates::instantiate("affinity-diagram"));
        let mut harness = Harness::builder()
            .with_size(vec2(1200.0, 900.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        let mut painted = false;
        for _ in 0..8 {
            harness.step();
            let Some(toolbar) = visible_topic_toolbar(&harness) else {
                continue;
            };
            if !harness.output().shapes.iter().any(|shape| {
                matches!(
                    &shape.shape,
                    egui::Shape::Text(text)
                        if text.galley.job.text == icons::SUBTOPIC
                            && toolbar.expand(2.0).contains(text.pos)
                )
            }) {
                continue;
            }
            painted = true;
            let doc = harness.state_mut().doc_mut().unwrap();
            let layout = doc.view.layout_for(&doc.doc);
            let sheet = doc.doc.sheet();
            let canvas = doc.view.canvas_rect();
            let origin = canvas.center() + doc.view.offset;
            for item in &thinoda_core::scene::build_scene(sheet, &layout).items {
                let thinoda_core::SceneId::Topic(id) = item.id else {
                    continue;
                };
                if sheet.topic(id).is_none_or(|topic| topic.card.is_none()) {
                    continue;
                }
                if let Some(bounds) = item.visible_bounds() {
                    let visible = Rect::from_min_max(
                        origin + vec2(bounds.min.x, bounds.min.y) * doc.view.zoom,
                        origin + vec2(bounds.max().x, bounds.max().y) * doc.view.zoom,
                    )
                    .intersect(canvas);
                    assert!(
                        !visible.is_positive() || !toolbar.intersects(visible),
                        "the first painted toolbar covers a card: {toolbar:?}, {visible:?}"
                    );
                }
            }
        }
        assert!(
            painted,
            "a safely placed toolbar should eventually be painted"
        );
    }

    #[test]
    fn affinity_heading_toolbar_does_not_cover_starter_cards() {
        for (collapsed, ai_enabled, ppp) in [
            (false, true, 1.0),
            (true, true, 2.0),
            (false, false, 2.0),
            (true, false, 1.0),
        ] {
            let mut app = ThinodaApp::headless();
            app.settings.ai.enabled = ai_enabled;
            app.new_doc(thinoda_core::templates::instantiate("affinity-diagram"));
            let mut harness = Harness::builder()
                .with_size(vec2(1200.0, 900.0))
                .with_pixels_per_point(ppp)
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            harness.run_steps(5);
            if !collapsed {
                harness.get_by_label("Expand board toolbar").click();
                harness.run_steps(3);
            }

            // Put the selected heading near the top and the starter notes just
            // below the frame titles, as in the Windows preview.
            {
                let doc = harness.state_mut().doc_mut().unwrap();
                let canvas = doc.view.canvas_rect();
                doc.view.zoom = 0.5;
                doc.view.offset = canvas.left_top() + vec2(120.0, 70.0) - canvas.center();
            }
            harness.run_steps(5);

            let toolbar = visible_topic_toolbar(&harness).expect("topic actions should fit");
            let board = harness
                .ctx
                .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_board_toolbar")))
                .unwrap();
            assert!(!toolbar.intersects(board));
            let editing = harness
                .ctx
                .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_editing_toolbar")))
                .unwrap();
            assert!(
                !toolbar.intersects(editing),
                "selected-topic actions must not cover the editing toolbar"
            );
            let heading = {
                let doc = harness.state_mut().doc_mut().unwrap();
                let heading = doc.view.primary().unwrap();
                assert_eq!(
                    doc.doc.sheet().topic(heading).unwrap().title,
                    "Group related ideas, then name each theme"
                );
                let layout = doc.view.layout_for(&doc.doc);
                let sheet = doc.doc.sheet();
                let scene = thinoda_core::scene::build_scene(sheet, &layout);
                let canvas = doc.view.canvas_rect();
                let origin = canvas.center() + doc.view.offset;
                let zoom = doc.view.zoom;
                assert!(canvas.contains_rect(toolbar));
                let mut visible_stickies = 0;
                for item in &scene.items {
                    let thinoda_core::SceneId::Topic(id) = item.id else {
                        continue;
                    };
                    let Some(card) = sheet.topic(id).and_then(|topic| topic.card) else {
                        continue;
                    };
                    let Some(bounds) = item.visible_bounds() else {
                        continue;
                    };
                    let max = bounds.max();
                    let visible = Rect::from_min_max(
                        origin + vec2(bounds.min.x, bounds.min.y) * zoom,
                        origin + vec2(max.x, max.y) * zoom,
                    )
                    .intersect(canvas);
                    if visible.is_positive() {
                        if card.kind == CardKind::Sticky {
                            visible_stickies += 1;
                        }
                        assert!(
                            !toolbar.intersects(visible.expand(0.5)),
                            "toolbar obscures card {id:?} when collapsed={collapsed}, ai={ai_enabled}, ppp={ppp}: {toolbar:?}, {visible:?}"
                        );
                    }
                }
                assert_eq!(visible_stickies, 4, "the starter notes must be visible");
                for zone in &sheet.zones {
                    let min = origin + vec2(zone.frame.min.x, zone.frame.min.y) * zoom;
                    let max = origin + vec2(zone.frame.max().x, zone.frame.max().y) * zoom;
                    let title =
                        Rect::from_min_max(min, egui::pos2(max.x, (min.y + 24.0).min(max.y)));
                    assert!(
                        !toolbar.intersects(title),
                        "toolbar obscures frame title {}: {toolbar:?}, {title:?}",
                        zone.name
                    );
                }
                heading
            };
            harness.get_by_label("Toggle to-do").click();
            harness.run_steps(3);
            assert_eq!(
                harness
                    .state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .topic(heading)
                    .unwrap()
                    .todo,
                Some(false),
                "the selected topic action remains clickable"
            );
            harness.get_by_label("Sticky note").click();
            harness.run_steps(3);
            assert_eq!(
                harness.state().doc().unwrap().view.board_tool,
                BoardTool::Sticky,
                "the board toolbox remains clickable"
            );
        }
    }

    #[test]
    fn board_toolbar_card_obstacles_respect_rotation_zoom_and_clipping() {
        let mut sheet = thinoda_core::Sheet::new("Board", "");
        sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
        let mut card = Card::new(CardKind::Sticky, Vec2::new(120.0, 40.0));
        card.rotation = 90.0;
        let id = sheet.add_card("Visible", Vec2::ZERO, card).unwrap();
        sheet
            .create_zone(
                "Clipped",
                vec![id],
                BoardRect::from_min_size(Vec2::new(-10.0, -100.0), Vec2::new(110.0, 200.0)),
            )
            .unwrap();
        sheet.zones[0].clip = true;
        sheet
            .add_card("Offscreen", Vec2::new(10_000.0, 10_000.0), card)
            .unwrap();
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let canvas = Rect::from_min_max(egui::pos2(190.0, 80.0), egui::pos2(260.0, 180.0));
        let obstacles =
            crate::ui::board_card_obstacles(&sheet, &layout, egui::pos2(200.0, 150.0), 2.0, canvas);
        assert_eq!(obstacles.len(), 1);
        let actual = obstacles[0];
        for (value, expected) in [
            (actual.left(), 190.0),
            (actual.right(), 240.0),
            (actual.top(), 80.0),
            (actual.bottom(), 180.0),
        ] {
            assert!((value - expected).abs() < 0.001, "{actual:?}");
        }
    }

    #[test]
    fn selected_rotated_card_toolbar_stays_outside_its_painted_bounds() {
        let mut app = ThinodaApp::headless();
        app.settings.ai.enabled = false;
        app.new_doc(thinoda_core::templates::instantiate("blank-canvas"));
        let selected = {
            let doc = app.doc_mut().unwrap();
            let selected = doc
                .doc
                .edit_sheet("Rotated card", |sheet| {
                    let mut card = Card::new(CardKind::Sticky, Vec2::new(120.0, 40.0));
                    card.rotation = 90.0;
                    sheet
                        .add_card("Selected", Vec2::ZERO, card)
                        .map_err(thinoda_core::EditError::Invalid)
                })
                .unwrap();
            doc.view.select_only(selected);
            selected
        };
        let mut harness = Harness::builder()
            .with_size(vec2(1000.0, 700.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(5);
        {
            let doc = harness.state_mut().doc_mut().unwrap();
            doc.view.zoom = 1.5;
            doc.view.offset = egui::Vec2::ZERO;
        }
        harness.run_steps(5);
        let toolbar = visible_topic_toolbar(&harness).expect("topic actions should fit");
        let doc = harness.state_mut().doc_mut().unwrap();
        let layout = doc.view.layout_for(&doc.doc);
        let scene = thinoda_core::scene::build_scene(doc.doc.sheet(), &layout);
        let bounds = scene
            .item(thinoda_core::SceneId::Topic(selected))
            .unwrap()
            .visible_bounds()
            .unwrap();
        let origin = doc.view.canvas_rect().center() + doc.view.offset;
        let max = bounds.max();
        let screen = Rect::from_min_max(
            origin + vec2(bounds.min.x, bounds.min.y) * doc.view.zoom,
            origin + vec2(max.x, max.y) * doc.view.zoom,
        );
        assert!(
            !toolbar.intersects(screen),
            "toolbar covers the rotated selected card: {toolbar:?}, {screen:?}"
        );
    }

    #[test]
    fn selected_card_toolbar_keeps_multiline_frame_titles_visible() {
        let mut app = ThinodaApp::headless();
        app.settings.ai.enabled = false;
        app.new_doc(thinoda_core::templates::instantiate("blank-canvas"));
        let name = "FIRST title line\nSECOND title line\nTHIRD title line\nFOURTH title line\nFIFTH title line\nSIXTH title line";
        {
            let doc = app.doc_mut().unwrap();
            let selected = doc
                .doc
                .edit_sheet("Card and multiline frame", |sheet| {
                    let selected = sheet
                        .add_card(
                            "Selected",
                            Vec2::ZERO,
                            Card::new(CardKind::Sticky, Vec2::new(120.0, 60.0)),
                        )
                        .map_err(thinoda_core::EditError::Invalid)?;
                    sheet
                        .create_board_frame(
                            name,
                            BoardRect::from_min_size(
                                Vec2::new(-80.0, -120.0),
                                Vec2::new(400.0, 200.0),
                            ),
                        )
                        .map_err(thinoda_core::EditError::Invalid)?;
                    Ok(selected)
                })
                .unwrap();
            doc.view.select_only(selected);
        }
        let mut harness = Harness::builder()
            .with_size(vec2(1000.0, 700.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(5);
        {
            let doc = harness.state_mut().doc_mut().unwrap();
            doc.view.zoom = 1.0;
            doc.view.offset = egui::Vec2::ZERO;
        }
        harness.run_steps(5);

        let toolbar = visible_topic_toolbar(&harness).expect("topic actions should fit");
        let doc = harness.state().doc().unwrap();
        let canvas = doc.view.canvas_rect();
        let origin = canvas.center() + doc.view.offset;
        let zone = &doc.doc.sheet().zones[0];
        let frame = Rect::from_min_size(
            origin + vec2(zone.frame.min.x, zone.frame.min.y) * doc.view.zoom,
            vec2(zone.frame.size.x, zone.frame.size.y) * doc.view.zoom,
        );
        let title_size = harness.ctx.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(
                    name.to_owned(),
                    egui::FontId::proportional(12.0),
                    Color32::WHITE,
                )
                .size()
        });
        let title = Rect::from_min_size(frame.left_top() + vec2(8.0, 6.0), title_size)
            .intersect(frame.shrink(4.0))
            .intersect(canvas);
        assert!(title.height() > 24.0, "test requires a multiline title");
        let topic = doc.view.selected_topic_rect(&doc.doc).unwrap();
        let legacy = Rect::from_min_size(
            egui::pos2(
                topic.center().x - toolbar.width() * 0.5,
                topic.top() - 12.0 - toolbar.height(),
            ),
            toolbar.size(),
        );
        assert!(legacy.intersects(title), "test must reproduce the overlap");
        assert!(!toolbar.intersects(title), "{toolbar:?}, {title:?}");
    }

    #[test]
    fn dense_board_search_finds_sideways_space_with_many_obstacles() {
        let available = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(500.0, 300.0));
        let desired = Rect::from_min_size(egui::pos2(200.0, 120.0), vec2(100.0, 30.0));
        let obstacles: Vec<_> = (0..10_000)
            .map(|index| {
                let offset = index as f32 * 0.001;
                Rect::from_min_max(
                    egui::pos2(180.0 + offset, 0.0),
                    egui::pos2(320.0 + offset, 300.0),
                )
            })
            .collect();
        let result = crate::ui::nearest_clear_toolbar_rect(desired, available, &obstacles, 1.0)
            .expect("there is room beside the blocked column");
        assert!(available.contains_rect(result));
        assert_ne!(result.left(), desired.left());
        assert!(
            obstacles
                .iter()
                .all(|obstacle| !result.intersects(*obstacle))
        );
        assert!(
            crate::ui::nearest_clear_toolbar_rect(desired, available, &[available], 1.0).is_none()
        );
    }

    #[test]
    fn toolbar_search_matches_exhaustive_geometry_on_small_boards() {
        fn next(seed: &mut u64, limit: u32) -> u32 {
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((*seed >> 32) as u32) % limit
        }
        fn overlaps(a: Rect, b: Rect) -> bool {
            a.left() < b.right()
                && a.right() > b.left()
                && a.top() < b.bottom()
                && a.bottom() > b.top()
        }

        let mut seed = 0x7468696e6f6461;
        for case in 0..2_000 {
            let width = next(&mut seed, 16) + 5;
            let height = next(&mut seed, 16) + 5;
            let available =
                Rect::from_min_size(egui::Pos2::ZERO, vec2(width as f32, height as f32));
            let size = vec2(
                (next(&mut seed, width) + 1) as f32,
                (next(&mut seed, height) + 1) as f32,
            );
            let desired = Rect::from_min_size(
                egui::pos2(
                    next(&mut seed, width + 20) as f32 - 10.0,
                    next(&mut seed, height + 20) as f32 - 10.0,
                ),
                size,
            );
            let clearance = next(&mut seed, 5) as f32 * 0.5;
            let count = next(&mut seed, 8);
            let obstacles: Vec<_> = (0..count)
                .map(|_| {
                    Rect::from_min_size(
                        egui::pos2(
                            next(&mut seed, width + 10) as f32 - 5.0,
                            next(&mut seed, height + 10) as f32 - 5.0,
                        ),
                        vec2(
                            (next(&mut seed, 12) + 1) as f32,
                            (next(&mut seed, 12) + 1) as f32,
                        ),
                    )
                })
                .collect();
            let max = available.max - size;
            let target = egui::pos2(
                desired.left().clamp(available.left(), max.x),
                desired.top().clamp(available.top(), max.y),
            );
            let mut xs = vec![available.left(), max.x, target.x];
            let mut ys = vec![available.top(), max.y, target.y];
            for obstacle in &obstacles {
                let blocked = Rect::from_min_max(
                    obstacle.min - size - vec2(clearance, clearance),
                    obstacle.max + vec2(clearance, clearance),
                );
                xs.extend(
                    [blocked.left(), blocked.right()]
                        .into_iter()
                        .filter(|x| available.left() <= *x && *x <= max.x),
                );
                ys.extend(
                    [blocked.top(), blocked.bottom()]
                        .into_iter()
                        .filter(|y| available.top() <= *y && *y <= max.y),
                );
            }
            let expected = xs
                .iter()
                .flat_map(|x| ys.iter().map(move |y| egui::pos2(*x, *y)))
                .map(|position| Rect::from_min_size(position, size))
                .filter(|rect| {
                    obstacles
                        .iter()
                        .all(|obstacle| !overlaps(*rect, obstacle.expand(clearance)))
                })
                .map(|rect| (rect.min - target).length_sq())
                .min_by(f32::total_cmp);
            let actual =
                crate::ui::nearest_clear_toolbar_rect(desired, available, &obstacles, clearance);
            assert_eq!(
                actual.is_some(),
                expected.is_some(),
                "case {case}: {desired:?}, {available:?}, {obstacles:?}, {clearance}"
            );
            if let (Some(rect), Some(distance)) = (actual, expected) {
                assert!(available.contains_rect(rect), "case {case}: {rect:?}");
                assert!(
                    obstacles
                        .iter()
                        .all(|obstacle| !overlaps(rect, obstacle.expand(clearance))),
                    "case {case}: {rect:?}, {obstacles:?}, {clearance}"
                );
                assert!(
                    ((rect.min - target).length_sq() - distance).abs() < 0.001,
                    "case {case}: {rect:?}, expected distance {distance}"
                );
            }
        }
    }

    #[test]
    fn dense_board_hides_only_the_toolbar_until_space_returns() {
        let mut app = ThinodaApp::headless();
        app.settings.ai.enabled = false;
        app.new_doc(thinoda_core::templates::instantiate("blank-canvas"));
        let (selected, blocker) = {
            let doc = app.doc_mut().unwrap();
            let ids = doc
                .doc
                .edit_sheet("Dense board", |sheet| {
                    let selected = sheet
                        .add_card(
                            "Selected",
                            Vec2::ZERO,
                            Card::new(CardKind::Sticky, Vec2::new(120.0, 60.0)),
                        )
                        .map_err(thinoda_core::EditError::Invalid)?;
                    let blocker = sheet
                        .add_card(
                            "Cover",
                            Vec2::ZERO,
                            Card::new(CardKind::Sticky, Vec2::new(4_000.0, 4_000.0)),
                        )
                        .map_err(thinoda_core::EditError::Invalid)?;
                    Ok((selected, blocker))
                })
                .unwrap();
            doc.view.select_only(ids.0);
            ids
        };
        let mut harness = Harness::builder()
            .with_size(vec2(1000.0, 700.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(5);
        {
            let doc = harness.state_mut().doc_mut().unwrap();
            doc.view.zoom = 1.0;
            doc.view.offset = egui::Vec2::ZERO;
        }
        harness.run_steps(5);
        assert!(visible_topic_toolbar(&harness).is_none());
        assert_eq!(
            harness.state().doc().unwrap().view.primary(),
            Some(selected)
        );
        assert!(
            harness
                .state()
                .command_available(&Cmd::Canvas(thinoda_canvas::Action::ToggleTodo))
        );
        harness.get_by_label(t("Main Menu")).click();
        harness.run_steps(2);
        harness.get_by_label("Insert").click();
        harness.run_steps(2);
        harness
            .get_by_role_and_label(egui::accesskit::Role::Button, "To-Do")
            .click();
        harness.run_steps(3);
        assert_eq!(
            harness
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .topic(selected)
                .unwrap()
                .todo,
            Some(false),
            "menu actions stay usable while the floating toolbar is hidden"
        );

        harness
            .state_mut()
            .doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Remove blocker", |sheet| {
                assert!(sheet.detach_subtree(blocker).is_some());
                Ok(())
            })
            .unwrap();
        harness.run_steps(5);
        assert!(visible_topic_toolbar(&harness).is_some());
        assert_eq!(
            harness.state().doc().unwrap().view.primary(),
            Some(selected)
        );
        harness.get_by_label("Toggle to-do").click();
        harness.run_steps(3);
        assert_eq!(
            harness
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .topic(selected)
                .unwrap()
                .todo,
            Some(true)
        );
    }

    #[test]
    fn hidden_board_toolbar_remeasures_when_actions_become_narrower() {
        let mut app = ThinodaApp::headless();
        app.settings.ai.enabled = true;
        app.new_doc(thinoda_core::templates::instantiate("blank-canvas"));
        let selected = {
            let doc = app.doc_mut().unwrap();
            let selected = doc
                .doc
                .edit_sheet("Selected card", |sheet| {
                    sheet
                        .add_card(
                            "Selected",
                            Vec2::ZERO,
                            Card::new(CardKind::Sticky, Vec2::new(120.0, 60.0)),
                        )
                        .map_err(thinoda_core::EditError::Invalid)
                })
                .unwrap();
            doc.view.select_only(selected);
            selected
        };
        let mut harness = Harness::builder()
            .with_size(vec2(1000.0, 700.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(5);
        {
            let doc = harness.state_mut().doc_mut().unwrap();
            doc.view.zoom = 1.0;
            doc.view.offset = egui::Vec2::ZERO;
        }
        harness.run_steps(5);
        let wide = visible_topic_toolbar(&harness).unwrap().size();
        harness.state_mut().settings.ai.enabled = false;
        harness.run_steps(5);
        let narrow = visible_topic_toolbar(&harness).unwrap().size();
        assert!(wide.x > narrow.x + 8.0);
        harness.state_mut().settings.ai.enabled = true;
        harness.run_steps(5);

        let canvas = harness.state().doc().unwrap().view.canvas_rect();
        let slot_size = vec2((wide.x + narrow.x) * 0.5, narrow.y + 8.0);
        let slot = Rect::from_min_size(
            egui::pos2(canvas.center().x - slot_size.x * 0.5, canvas.top() + 80.0),
            slot_size,
        );
        let blockers = [
            Rect::from_min_max(canvas.min, egui::pos2(canvas.right(), slot.top())),
            Rect::from_min_max(egui::pos2(canvas.left(), slot.bottom()), canvas.max),
            Rect::from_min_max(
                egui::pos2(canvas.left(), slot.top()),
                egui::pos2(slot.left(), slot.bottom()),
            ),
            Rect::from_min_max(
                egui::pos2(slot.right(), slot.top()),
                egui::pos2(canvas.right(), slot.bottom()),
            ),
        ];
        let origin = canvas.center();
        harness
            .state_mut()
            .doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Leave a narrow slot", |sheet| {
                for rect in blockers {
                    let center = rect.center() - origin;
                    let size = rect.size();
                    sheet
                        .add_card(
                            "Obstacle",
                            Vec2::new(center.x, center.y),
                            Card::new(CardKind::Sticky, Vec2::new(size.x, size.y)),
                        )
                        .map_err(thinoda_core::EditError::Invalid)?;
                }
                Ok(())
            })
            .unwrap();
        harness.run_steps(5);
        assert!(visible_topic_toolbar(&harness).is_none());
        assert_eq!(
            harness.state().doc().unwrap().view.primary(),
            Some(selected)
        );

        harness.state_mut().settings.ai.enabled = false;
        harness.run_steps(5);
        let toolbar = visible_topic_toolbar(&harness).expect("the narrower toolbar fits the slot");
        assert!(slot.contains_rect(toolbar), "{slot:?}, {toolbar:?}");
        assert_eq!(
            harness.state().doc().unwrap().view.primary(),
            Some(selected)
        );
    }

    #[test]
    fn board_toolbar_disables_an_unexpected_size_change_until_reflow() {
        let mut app = ThinodaApp::headless();
        app.settings.ai.enabled = false;
        app.new_doc(thinoda_core::templates::instantiate("blank-canvas"));
        {
            let doc = app.doc_mut().unwrap();
            let selected = doc
                .doc
                .edit_sheet("Selected card", |sheet| {
                    sheet
                        .add_card(
                            "Selected",
                            Vec2::ZERO,
                            Card::new(CardKind::Sticky, Vec2::new(120.0, 60.0)),
                        )
                        .map_err(thinoda_core::EditError::Invalid)
                })
                .unwrap();
            doc.view.select_only(selected);
        }
        let mut harness = Harness::builder()
            .with_size(vec2(1000.0, 700.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(5);

        let id = egui::Id::unique("thinoda_topic_toolbar");
        let layer = egui::LayerId::new(egui::Order::Middle, id);
        let mut state = egui::containers::AreaState::load(&harness.ctx, id).unwrap();
        let size = state.size.unwrap();
        state.size = Some(vec2(size.x * 0.5, size.y));
        harness
            .ctx
            .memory_mut(|memory| memory.areas_mut().set_state(layer, state));
        // A discard is not guaranteed to be granted by egui.
        harness
            .ctx
            .options_mut(|options| options.max_passes = std::num::NonZeroUsize::new(1).unwrap());
        harness.step();
        assert!(
            !egui::containers::AreaState::load(&harness.ctx, id)
                .unwrap()
                .interactable,
            "a stale-sized area must not intercept clicks"
        );

        harness.run_steps(3);
        assert!(
            egui::containers::AreaState::load(&harness.ctx, id)
                .unwrap()
                .interactable
        );
        let toolbar = visible_topic_toolbar(&harness).expect("the toolbar should reflow");
        let doc = harness.state().doc().unwrap();
        let card = doc.view.selected_topic_rect(&doc.doc).unwrap();
        assert!(!toolbar.intersects(card));
    }

    #[test]
    fn plain_map_topic_toolbar_keeps_its_legacy_anchor() {
        let mut app = ThinodaApp::headless();
        app.settings.ai.enabled = false;
        let mut harness = Harness::builder()
            .with_size(vec2(1200.0, 900.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(5);
        let doc = harness.state().doc().unwrap();
        assert!(!doc.doc.sheet().uses_board_semantics());
        let topic = doc.view.selected_topic_rect(&doc.doc).unwrap();
        let toolbar = visible_topic_toolbar(&harness).unwrap();
        let below = topic.top() - 52.0 < doc.view.canvas_rect().top();
        let expected = if below {
            topic.bottom() + 12.0
        } else {
            topic.top() - 12.0
        };
        let actual = if below {
            toolbar.top()
        } else {
            toolbar.bottom()
        };
        assert!((actual - expected).abs() <= 1.0, "{toolbar:?}, {topic:?}");
    }

    #[test]
    fn choosing_ink_reveals_its_context_after_scrolling() {
        let mut harness = Harness::builder()
            .with_size(vec2(500.0, 300.0))
            .build_ui_state(expanded_toolbar, ThinodaApp::headless());
        harness.run_steps(3);
        click_scrolled_into_view(&mut harness, "Pen");
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Pen
        );
        let color = harness.get_by_label("Ink color").rect();
        assert!(harness.ctx.content_rect().contains_rect(color), "{color:?}");
        harness.get_by_label("Ink color").click();
        harness.run_steps(2);
        assert!(harness.query_by_label("Auto").is_some());
    }

    #[cfg(feature = "gpu-tests")]
    #[test]
    fn toolbox_visual_previews() {
        let Some(directory) = std::env::var_os("THINODA_BOARD_VISUAL_DIR") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        for dark in [false, true] {
            let mut app = ThinodaApp::headless();
            app.settings.theme = if dark {
                thinoda_platform::settings::ThemeMode::Dark
            } else {
                thinoda_platform::settings::ThemeMode::Light
            };
            let mut harness = Harness::builder()
                .wgpu()
                .with_size(vec2(1200.0, 900.0))
                .build_ui_state(
                    |ui, app: &mut ThinodaApp| {
                        start_expanded(ui.ctx(), app);
                        app.draw(ui);
                    },
                    app,
                );
            harness.run_steps(5);
            let theme = if dark { "dark" } else { "light" };
            harness
                .render()
                .expect("board toolbox render")
                .save(directory.join(format!("board-toolbox-{theme}-expanded.png")))
                .unwrap();
            harness.get_by_label("Collapse board toolbar").click();
            harness.run_steps(3);
            click_scrolled_into_view(&mut harness, "Pen");
            harness
                .render()
                .expect("collapsed ink toolbox render")
                .save(directory.join(format!("board-toolbox-{theme}-ink.png")))
                .unwrap();

            let mut board = ThinodaApp::headless();
            board.settings.theme = if dark {
                thinoda_platform::settings::ThemeMode::Dark
            } else {
                thinoda_platform::settings::ThemeMode::Light
            };
            board.new_doc(thinoda_core::templates::instantiate("kanban-board"));
            let mut board = Harness::builder()
                .wgpu()
                .with_size(vec2(1200.0, 900.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), board);
            board.run_steps(5);
            assert!(board.query_by_label("Expand board toolbar").is_some());
            board
                .render()
                .expect("collapsed freeform board toolbox render")
                .save(directory.join(format!("board-toolbox-{theme}-board-collapsed.png")))
                .unwrap();
            board.get_by_label("Expand board toolbar").click();
            board.run_steps(3);
            board
                .render()
                .expect("freeform board toolbox render")
                .save(directory.join(format!("board-toolbox-{theme}-board.png")))
                .unwrap();
        }
    }

    #[test]
    fn short_toolbox_scrolls_within_the_canvas_in_both_themes_and_scales() {
        for dark in [false, true] {
            for scale in [1.0, 2.0, 3.0] {
                for collapsed in [false, true] {
                    let mut app = ThinodaApp::headless();
                    app.doc_mut().unwrap().view.dark = dark;
                    let mut harness = Harness::builder()
                        .with_size(vec2(320.0, 240.0) / scale)
                        .build_ui_state(
                            move |ui, app: &mut ThinodaApp| {
                                ui.ctx().set_visuals(if dark {
                                    egui::Visuals::dark()
                                } else {
                                    egui::Visuals::light()
                                });
                                expanded_toolbar(ui, app);
                            },
                            app,
                        );
                    harness.run_steps(3);
                    if collapsed {
                        harness.get_by_label("Collapse board toolbar").click();
                        harness.run_steps(2);
                    }
                    let toggle = if collapsed {
                        "Expand board toolbar"
                    } else {
                        "Collapse board toolbar"
                    };
                    assert!(
                        harness.ctx.content_rect().contains_rect(
                            harness
                                .get_by_role_and_label(egui::Role::Button, toggle)
                                .rect()
                        ),
                        "toggle outside canvas: dark={dark} scale={scale} collapsed={collapsed}"
                    );
                    let scroll_point =
                        harness.get_by_label("Select tool").rect().min + vec2(5.0, 5.0);
                    harness.event(egui::Event::PointerMoved(scroll_point));
                    harness.event(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0.0, -1800.0),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Unknown,
                        modifiers: egui::Modifiers::NONE,
                    });
                    harness.run_steps(4);
                    let minimap = harness.get_by_label("Board minimap").rect();
                    assert!(
                        harness.ctx.content_rect().contains_rect(minimap),
                        "last action unreachable: dark={dark} scale={scale} collapsed={collapsed}, \
                         button={minimap:?}, canvas={:?}",
                        harness.ctx.content_rect()
                    );
                    harness.get_by_label("Board minimap").click();
                    harness.run_steps(2);
                    assert!(
                        harness.state().doc().unwrap().view.show_minimap,
                        "last action not clickable: dark={dark} scale={scale} collapsed={collapsed}, \
                         button={minimap:?}, scroll start={scroll_point:?}, toggle={:?}",
                        harness.get_by_label(toggle).rect()
                    );
                }
            }
        }
    }

    #[test]
    fn zoom_to_selection_fits_expanded_mixed_group_without_document_edits() {
        let mut harness = harness();
        let (card, object) = {
            let doc = harness.state_mut().doc_mut().unwrap();
            doc.doc
                .edit_sheet("Group", |sheet| {
                    let card = sheet
                        .add_card(
                            "Card",
                            Vec2::new(500.0, 300.0),
                            Card::new(CardKind::Sticky, Vec2::new(140.0, 100.0)),
                        )
                        .map_err(thinoda_core::EditError::Invalid)?;
                    let object = sheet
                        .add_canvas_object(CanvasObject::shape(
                            ShapeKind::Rectangle,
                            BoardRect::from_center_size(
                                Vec2::new(760.0, 300.0),
                                Vec2::new(120.0, 80.0),
                            ),
                        ))
                        .map_err(thinoda_core::EditError::Invalid)?;
                    sheet
                        .add_canvas_object(CanvasObject::shape(
                            ShapeKind::Rectangle,
                            BoardRect::from_center_size(
                                Vec2::new(-2000.0, -2000.0),
                                Vec2::new(100.0, 100.0),
                            ),
                        ))
                        .map_err(thinoda_core::EditError::Invalid)?;
                    sheet
                        .group_board_members(&[
                            thinoda_core::board::BoardMember::Card(card),
                            thinoda_core::board::BoardMember::Object(object),
                        ])
                        .map_err(thinoda_core::EditError::Invalid)?;
                    Ok((card, object))
                })
                .unwrap()
        };
        harness
            .state_mut()
            .doc_mut()
            .unwrap()
            .view
            .select_board_object(object);
        harness.run_steps(2);
        let before = harness.state().doc().unwrap().doc.workbook().clone();
        let bounds = {
            let doc = harness.state_mut().doc_mut().unwrap();
            let layout = doc.view.layout_for(&doc.doc);
            let scene = thinoda_core::scene::build_scene(doc.doc.sheet(), &layout);
            scene
                .item(thinoda_core::SceneId::Topic(card))
                .unwrap()
                .visible_bounds()
                .unwrap()
                .union(
                    &scene
                        .item(thinoda_core::SceneId::Object(object))
                        .unwrap()
                        .visible_bounds()
                        .unwrap(),
                )
        };
        let ctx = harness.ctx.clone();
        harness.state_mut().exec(&ctx, Cmd::BoardZoomSelection);
        let doc = harness.state().doc().unwrap();
        let area = doc.view.canvas_rect().shrink(40.0);
        let center = bounds.center();
        let screen_center = doc.view.canvas_rect().center()
            + doc.view.offset
            + vec2(center.x, center.y) * doc.view.zoom;
        assert!(screen_center.distance(area.center()) < 0.1);
        assert!(bounds.size.x * doc.view.zoom <= area.width() + 0.1);
        assert!(bounds.size.y * doc.view.zoom <= area.height() + 0.1);
        assert_eq!(doc.doc.workbook(), &before);
    }

    #[test]
    fn zoom_to_selection_handles_topics_and_frames_without_board_members() {
        let mut harness = harness();
        let root = harness.state().doc().unwrap().doc.sheet().root;
        harness.state_mut().doc_mut().unwrap().view.selection = vec![root];
        let ctx = harness.ctx.clone();
        harness.state_mut().exec(&ctx, Cmd::BoardZoomSelection);
        assert!(harness.state().doc().unwrap().view.zoom > 1.0);

        let frame = BoardRect::from_center_size(Vec2::new(900.0, 400.0), Vec2::new(300.0, 200.0));
        let id = harness
            .state_mut()
            .doc_mut()
            .unwrap()
            .doc
            .create_board_frame("Frame", frame)
            .unwrap();
        harness
            .state_mut()
            .doc_mut()
            .unwrap()
            .view
            .select_object(thinoda_canvas::ObjectSelection::Zone(id));
        harness.run_steps(2);
        harness.state_mut().exec(&ctx, Cmd::BoardZoomSelection);
        let view = &harness.state().doc().unwrap().view;
        let center = frame.center();
        let screen_center =
            view.canvas_rect().center() + view.offset + vec2(center.x, center.y) * view.zoom;
        assert!(screen_center.distance(view.canvas_rect().center()) < 0.1);
    }

    #[test]
    fn object_description_is_editable_and_undoable() {
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let id = doc
            .doc
            .edit_sheet("Shape", |sheet| {
                sheet
                    .add_canvas_object(CanvasObject::shape(
                        ShapeKind::Rectangle,
                        BoardRect::from_center_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
                    ))
                    .map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        doc.view.select_board_object(id);
        let mut harness = Harness::builder()
            .with_size(vec2(450.0, 700.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    app.board_object_properties(ui);
                },
                app,
            );
        harness.run_steps(3);
        harness
            .get_by_role_and_label(egui::Role::MultilineTextInput, "Description")
            .click();
        harness.run_steps(2);
        harness
            .get_by_role_and_label(egui::Role::MultilineTextInput, "Description")
            .type_text("Accessible description");
        harness.run_steps(2);
        assert_eq!(
            harness
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .alt_text,
            "Accessible description"
        );
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(
            harness
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .alt_text,
            ""
        );
    }

    #[test]
    fn custom_shape_fill_can_be_enabled_and_undone() {
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let id = doc
            .doc
            .edit_sheet("Shape", |sheet| {
                sheet
                    .add_canvas_object(CanvasObject::shape(
                        ShapeKind::Rectangle,
                        BoardRect::from_center_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
                    ))
                    .map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        doc.view.select_board_object(id);
        let mut harness = Harness::builder()
            .with_size(vec2(450.0, 700.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    app.board_object_properties(ui);
                },
                app,
            );
        harness.run_steps(3);
        harness
            .get_by_role_and_label(egui::Role::CheckBox, "Fill")
            .click();
        harness.run_steps(3);
        assert_eq!(
            harness
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .style
                .fill,
            Some(Color::WHITE)
        );
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(
            harness
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .style
                .fill,
            None
        );
    }

    #[test]
    fn connector_marker_inspector_is_accessible_and_undoable() {
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        let id = doc
            .doc
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        doc.view.select_board_object(id);
        let mut harness = Harness::builder()
            .with_size(vec2(450.0, 700.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    app.board_object_properties(ui);
                },
                app,
            );
        harness.run_steps(3);
        assert!(
            harness
                .get_by_label("Start arrow style")
                .accesskit_node()
                .is_disabled()
        );
        harness.get_by_label("End arrow style").click();
        harness.run_steps(3);
        harness.get_by_label("Block arrow").click();
        harness.run_steps(3);
        let current = &harness
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .kind;
        let CanvasObjectKind::Connector(current) = current else {
            panic!("expected connector");
        };
        assert_eq!(current.end_marker.unwrap().kind, ConnectorMarkerKind::Block);
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        harness.run_steps(2);
        let current = &harness
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .kind;
        let CanvasObjectKind::Connector(current) = current else {
            panic!("expected connector");
        };
        assert_eq!(current.end_marker.unwrap().kind, ConnectorMarkerKind::Open);
        harness
            .get_by_role_and_label(egui::Role::CheckBox, "Arrow at end")
            .click();
        harness.run_steps(3);
        let current = &harness
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .kind;
        let CanvasObjectKind::Connector(current) = current else {
            panic!("expected connector");
        };
        assert!(!current.arrow_end);
        assert_eq!(current.end_marker, None);
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        let current = &harness
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .kind;
        let CanvasObjectKind::Connector(current) = current else {
            panic!("expected connector");
        };
        assert_eq!(current.end_marker.unwrap().kind, ConnectorMarkerKind::Open);
    }

    #[test]
    fn connector_inspector_selects_classic_thin_and_legacy_arrows() {
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        let id = doc
            .doc
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        doc.view.select_board_object(id);
        let mut harness = Harness::builder()
            .with_size(vec2(450.0, 700.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    app.board_object_properties(ui);
                },
                app,
            );
        harness.run_steps(3);
        for (label, kind) in [
            ("Classic arrow", Some(ConnectorMarkerKind::Classic)),
            ("Thin classic arrow", Some(ConnectorMarkerKind::ClassicThin)),
            ("Thin open arrow", Some(ConnectorMarkerKind::OpenThin)),
            ("Thin block arrow", Some(ConnectorMarkerKind::BlockThin)),
            ("Diamond", Some(ConnectorMarkerKind::Diamond)),
            ("Thin diamond", Some(ConnectorMarkerKind::DiamondThin)),
            ("Oval", Some(ConnectorMarkerKind::Oval)),
            ("Half circle", Some(ConnectorMarkerKind::HalfCircle)),
            ("Legacy triangle", None),
        ] {
            harness.get_by_label("End arrow style").click();
            harness.run_steps(3);
            harness.get_by_label(label).scroll_to_me();
            harness.run_steps(3);
            harness.get_by_label(label).click();
            harness.run_steps(3);
            let CanvasObjectKind::Connector(current) = &harness
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .kind
            else {
                panic!("expected connector");
            };
            assert_eq!(current.end_marker.map(|marker| marker.kind), kind);
        }
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        let CanvasObjectKind::Connector(current) = &harness
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .kind
        else {
            panic!("expected connector");
        };
        assert_eq!(
            current.end_marker.unwrap().kind,
            ConnectorMarkerKind::HalfCircle
        );
    }

    #[test]
    fn connector_marker_menu_scrolls_to_half_circle_with_the_mouse() {
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        let id = doc
            .doc
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        doc.view.select_board_object(id);
        let mut harness = Harness::builder()
            .with_size(vec2(450.0, 700.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    app.board_object_properties(ui);
                },
                app,
            );
        harness.run_steps(3);
        harness.get_by_label("End arrow style").click();
        harness.run_steps(3);
        let before = harness.get_by_label("Half circle").rect();
        harness.hover_at(harness.get_by_label("Legacy triangle").rect().center());
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -1200.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(15);
        let after = harness.get_by_label("Half circle").rect();
        assert!(
            after.top() < before.top(),
            "before={before:?}, after={after:?}"
        );
        assert!(harness.ctx.content_rect().contains(after.center()));
        harness.get_by_label("Half circle").click();
        harness.run_steps(3);
        let CanvasObjectKind::Connector(current) = &harness
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .kind
        else {
            panic!("expected connector");
        };
        assert_eq!(
            current.end_marker.unwrap().kind,
            ConnectorMarkerKind::HalfCircle
        );
    }

    #[test]
    fn connector_marker_size_inspector_is_accessible_and_undoable() {
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        let id = doc
            .doc
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        doc.view.select_board_object(id);
        let mut harness = Harness::builder()
            .with_size(vec2(450.0, 700.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    app.board_object_properties(ui);
                },
                app,
            );
        harness.run_steps(3);
        assert!(
            harness
                .get_by_label("Start arrow size")
                .accesskit_node()
                .is_disabled()
        );
        harness.get_by_label("End arrow size").click();
        harness.run_steps(2);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness.get_by_label("End arrow size").type_text("12.5");
        harness.key_press(egui::Key::Enter);
        harness.run_steps(3);
        let current = |app: &ThinodaApp| {
            let CanvasObjectKind::Connector(connector) = &app
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .kind
            else {
                panic!("expected connector");
            };
            connector.clone()
        };
        assert_eq!(current(harness.state()).end_marker_size, Some(12.5));
        harness.get_by_label("End arrow style").click();
        harness.run_steps(3);
        harness.get_by_label("Classic arrow").click();
        harness.run_steps(3);
        assert_eq!(current(harness.state()).end_marker_size, Some(12.5));
        harness.get_by_label("End arrow style").click();
        harness.run_steps(3);
        harness.get_by_label("Legacy triangle").click();
        harness.run_steps(3);
        assert_eq!(current(harness.state()).end_marker_size, None);
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        harness.run_steps(3);
        assert_eq!(current(harness.state()).end_marker_size, Some(12.5));
        harness
            .get_by_role_and_label(egui::Role::CheckBox, "Arrow at end")
            .click();
        harness.run_steps(3);
        assert_eq!(current(harness.state()).end_marker_size, None);
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(current(harness.state()).end_marker_size, Some(12.5));
        harness.run_steps(3);
        for (text, expected) in [
            ("6", None),
            ("-10", Some(0.0)),
            ("1000", Some(MAX_CONNECTOR_MARKER_SIZE)),
        ] {
            harness.get_by_label("End arrow size").click();
            harness.run_steps(2);
            harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
            harness.get_by_label("End arrow size").type_text(text);
            harness.key_press(egui::Key::Enter);
            harness.run_steps(3);
            assert_eq!(current(harness.state()).end_marker_size, expected, "{text}");
        }
    }

    #[test]
    fn connector_marker_fill_inspector_is_independent_and_undoable() {
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.arrow_start = true;
        connector.start_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Block,
        });
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Classic,
        });
        let id = doc
            .doc
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        doc.view.select_board_object(id);
        let mut harness = Harness::builder()
            .with_size(vec2(450.0, 700.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    app.board_object_properties(ui);
                },
                app,
            );
        harness.run_steps(3);
        let current = |app: &ThinodaApp| {
            let CanvasObjectKind::Connector(connector) = &app
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .kind
            else {
                panic!("expected connector");
            };
            connector.clone()
        };
        for label in ["Fill start arrow", "Fill end arrow"] {
            assert!(!harness.get_by_label(label).accesskit_node().is_disabled());
        }
        harness.get_by_label("Fill start arrow").click();
        harness.run_steps(3);
        assert!(current(harness.state()).start_marker_unfilled);
        assert!(!current(harness.state()).end_marker_unfilled);
        harness.get_by_label("Fill end arrow").click();
        harness.run_steps(3);
        assert!(current(harness.state()).end_marker_unfilled);
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        harness.run_steps(3);
        assert!(current(harness.state()).start_marker_unfilled);
        assert!(!current(harness.state()).end_marker_unfilled);
        harness.get_by_label("Fill end arrow").click();
        harness.run_steps(3);

        harness.get_by_label("End arrow style").click();
        harness.run_steps(3);
        harness.get_by_label("Open arrow").click();
        harness.run_steps(3);
        assert!(
            harness
                .get_by_label("Fill end arrow")
                .accesskit_node()
                .is_disabled()
        );
        assert!(current(harness.state()).end_marker_unfilled);
        harness.get_by_label("End arrow style").click();
        harness.run_steps(3);
        harness.get_by_label("Oval").scroll_to_me();
        harness.run_steps(3);
        harness.get_by_label("Oval").click();
        harness.run_steps(3);
        assert!(
            !harness
                .get_by_label("Fill end arrow")
                .accesskit_node()
                .is_disabled()
        );
        assert!(current(harness.state()).end_marker_unfilled);
        assert_eq!(
            current(harness.state()).end_marker.unwrap().kind,
            ConnectorMarkerKind::Oval
        );

        harness.get_by_label("End arrow style").click();
        harness.run_steps(3);
        harness.get_by_label("Half circle").scroll_to_me();
        harness.run_steps(3);
        harness.get_by_label("Half circle").click();
        harness.run_steps(3);
        assert!(
            harness
                .get_by_label("Fill end arrow")
                .accesskit_node()
                .is_disabled()
        );
        assert!(current(harness.state()).end_marker_unfilled);
        assert_eq!(
            current(harness.state()).end_marker.unwrap().kind,
            ConnectorMarkerKind::HalfCircle
        );
        harness.get_by_label("End arrow style").click();
        harness.run_steps(3);
        harness.get_by_label("Oval").scroll_to_me();
        harness.run_steps(3);
        harness.get_by_label("Oval").click();
        harness.run_steps(3);
        assert_eq!(
            current(harness.state()).end_marker.unwrap().kind,
            ConnectorMarkerKind::Oval
        );
        assert!(current(harness.state()).end_marker_unfilled);
        assert!(
            !harness
                .get_by_label("Fill end arrow")
                .accesskit_node()
                .is_disabled()
        );
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        harness.run_steps(3);
        assert_eq!(
            current(harness.state()).end_marker.unwrap().kind,
            ConnectorMarkerKind::HalfCircle
        );
        assert!(current(harness.state()).end_marker_unfilled);
        assert!(
            harness
                .get_by_label("Fill end arrow")
                .accesskit_node()
                .is_disabled()
        );

        harness.get_by_label("End arrow style").click();
        harness.run_steps(3);
        harness.get_by_label("Legacy triangle").scroll_to_me();
        harness.run_steps(3);
        harness.get_by_label("Legacy triangle").click();
        harness.run_steps(3);
        assert!(
            harness
                .get_by_label("Fill end arrow")
                .accesskit_node()
                .is_disabled()
        );
        assert!(!current(harness.state()).end_marker_unfilled);
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        harness.run_steps(3);
        assert!(current(harness.state()).end_marker_unfilled);
        harness
            .get_by_role_and_label(egui::Role::CheckBox, "Arrow at end")
            .click();
        harness.run_steps(3);
        assert!(!current(harness.state()).end_marker_unfilled);
        assert!(
            harness
                .get_by_label("Fill end arrow")
                .accesskit_node()
                .is_disabled()
        );
        assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
        assert!(current(harness.state()).end_marker_unfilled);
    }

    #[test]
    fn object_styles_copy_atomically_without_copying_content_or_geometry() {
        let mut app = ThinodaApp::headless();
        let ctx = Context::default();
        let doc = app.doc_mut().unwrap();
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            BoardRect::from_center_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
        );
        shape.style.fill = Some(Color::rgba(20, 40, 60, 255));
        shape.style.opacity = 0.6;
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = "Source".into();
        }
        let source = doc.doc.add_canvas_object(shape).unwrap();
        let targets = [150.0, 300.0].map(|x| {
            let mut shape = CanvasObject::shape(
                ShapeKind::Ellipse,
                BoardRect::from_center_size(Vec2::new(x, 0.0), Vec2::new(60.0, 40.0)),
            );
            shape.alt_text = "Keep this description".into();
            doc.doc.add_canvas_object(shape).unwrap()
        });
        doc.view.select_board_object(source);
        app.exec(&ctx, Cmd::CopyStyle);
        let doc = app.doc_mut().unwrap();
        doc.view.board_selection = targets.to_vec();
        let before = doc.doc.sheet().clone();
        assert!(app.command_available(&Cmd::PasteStyle));
        app.exec(&ctx, Cmd::PasteStyle);
        let doc = app.doc().unwrap();
        for id in targets {
            let object = doc.doc.sheet().canvas.object(id).unwrap();
            assert_eq!(object.style.fill, Some(Color::rgba(20, 40, 60, 255)));
            assert_eq!(object.style.opacity, 0.6);
            assert_eq!(object.rect, before.canvas.object(id).unwrap().rect);
            assert_eq!(object.alt_text, "Keep this description");
            assert!(matches!(
                object.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::Ellipse,
                    ..
                }
            ));
        }
        assert!(app.doc_mut().unwrap().doc.undo());
        assert_eq!(app.doc().unwrap().doc.sheet(), &before);
    }

    #[test]
    fn object_style_paste_rejects_incompatible_and_locked_targets() {
        let mut app = ThinodaApp::headless();
        let ctx = Context::default();
        let doc = app.doc_mut().unwrap();
        let source = doc
            .doc
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                BoardRect::from_center_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
            ))
            .unwrap();
        let target = doc
            .doc
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Ellipse,
                BoardRect::from_center_size(Vec2::new(200.0, 0.0), Vec2::new(120.0, 80.0)),
            ))
            .unwrap();
        let connector = doc
            .doc
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(200.0, 0.0)),
            )))
            .unwrap();
        doc.view.select_board_object(source);
        app.exec(&ctx, Cmd::CopyStyle);
        app.doc_mut().unwrap().view.select_board_object(connector);
        assert!(!app.command_available(&Cmd::PasteStyle));
        let doc = app.doc_mut().unwrap();
        doc.doc.set_canvas_objects_locked(&[target], true).unwrap();
        doc.view.select_board_object(target);
        assert!(!app.command_available(&Cmd::PasteStyle));
    }

    #[test]
    fn connector_and_ink_style_copy_keeps_endpoints_labels_and_samples() {
        let mut app = ThinodaApp::headless();
        let ctx = Context::default();
        let doc = app.doc_mut().unwrap();
        let mut source = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        source.route = ConnectorRoute::Curve;
        source.arrow_start = true;
        source.start_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Block,
        });
        source.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        source.start_marker_size = Some(12.5);
        source.end_marker_size = Some(0.0);
        source.start_marker_unfilled = true;
        source.end_marker_unfilled = true;
        source.color = Some(Color::rgba(30, 60, 90, 255));
        source.width = 4.0;
        let source = doc
            .doc
            .add_canvas_object(CanvasObject::connector(source))
            .unwrap();
        let mut target = CanvasConnector::new(
            Endpoint::Point(Vec2::new(0.0, 100.0)),
            Endpoint::Point(Vec2::new(100.0, 100.0)),
        );
        target.label = "Keep connector".into();
        let target = doc
            .doc
            .add_canvas_object(CanvasObject::connector(target))
            .unwrap();
        let ink_source = doc
            .doc
            .add_canvas_object(CanvasObject::ink(InkStroke::new(
                InkTool::Highlighter,
                Color::rgba(200, 80, 20, 255),
                7.0,
                vec![Vec2::ZERO, Vec2::new(50.0, 50.0)],
            )))
            .unwrap();
        let ink_target = doc
            .doc
            .add_canvas_object(CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::BLACK,
                2.0,
                vec![Vec2::new(300.0, 0.0), Vec2::new(350.0, 50.0)],
            )))
            .unwrap();
        let before_target = doc.doc.sheet().canvas.object(target).unwrap().clone();
        let before_ink = doc.doc.sheet().canvas.object(ink_target).unwrap().clone();
        doc.view.select_board_object(source);
        app.exec(&ctx, Cmd::CopyStyle);
        app.doc_mut().unwrap().view.select_board_object(target);
        app.exec(&ctx, Cmd::PasteStyle);
        let CanvasObjectKind::Connector(connector) = &app
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(target)
            .unwrap()
            .kind
        else {
            panic!("expected connector");
        };
        let CanvasObjectKind::Connector(before) = &before_target.kind else {
            panic!("expected connector");
        };
        assert_eq!(
            (connector.from, connector.to, &connector.label),
            (before.from, before.to, &before.label)
        );
        assert_eq!(connector.route, ConnectorRoute::Curve);
        assert!(connector.arrow_start);
        assert_eq!(
            connector.start_marker.unwrap().kind,
            ConnectorMarkerKind::Block
        );
        assert_eq!(
            connector.end_marker.unwrap().kind,
            ConnectorMarkerKind::Open
        );
        assert_eq!(connector.start_marker_size, Some(12.5));
        assert_eq!(connector.end_marker_size, Some(0.0));
        assert!(connector.start_marker_unfilled);
        assert!(connector.end_marker_unfilled);
        assert_eq!(connector.width, 4.0);
        assert!(app.doc_mut().unwrap().doc.undo());
        assert_eq!(
            app.doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(target)
                .unwrap(),
            &before_target
        );
        assert!(app.doc_mut().unwrap().doc.redo());
        app.doc_mut().unwrap().view.select_board_object(ink_source);
        app.exec(&ctx, Cmd::CopyStyle);
        app.doc_mut().unwrap().view.select_board_object(ink_target);
        app.exec(&ctx, Cmd::PasteStyle);
        let CanvasObjectKind::Ink(ink) = &app
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(ink_target)
            .unwrap()
            .kind
        else {
            panic!("expected ink");
        };
        let CanvasObjectKind::Ink(before) = &before_ink.kind else {
            panic!("expected ink");
        };
        assert_eq!(ink.points, before.points);
        assert_eq!(ink.tool, InkTool::Highlighter);
        assert_eq!(ink.width, 7.0);
        assert_eq!(ink.color, Color::rgba(200, 80, 20, 255));
    }

    #[test]
    fn card_style_copy_preserves_dimensions_and_is_undoable() {
        let mut app = ThinodaApp::headless();
        let ctx = Context::default();
        let doc = app.doc_mut().unwrap();
        let (source, target) = doc
            .doc
            .edit_sheet("Cards", |sheet| {
                let source = sheet.add_floating("Source", Vec2::ZERO);
                let target = sheet.add_floating("Target", Vec2::new(200.0, 0.0));
                let mut first = Card::new(CardKind::Sticky, Vec2::new(100.0, 100.0));
                first.color = Some(Color::rgba(20, 40, 60, 255));
                sheet
                    .set_topic_card(source, Some(first))
                    .map_err(thinoda_core::EditError::Invalid)?;
                sheet
                    .set_topic_card(
                        target,
                        Some(Card::new(CardKind::Sticky, Vec2::new(150.0, 90.0))),
                    )
                    .map_err(thinoda_core::EditError::Invalid)?;
                Ok((source, target))
            })
            .unwrap();
        doc.view.select_only(source);
        app.exec(&ctx, Cmd::CopyStyle);
        let doc = app.doc_mut().unwrap();
        doc.view.select_only(target);
        let before = doc.doc.sheet().clone();
        app.exec(&ctx, Cmd::PasteStyle);
        let card = app
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(target)
            .unwrap()
            .card
            .unwrap();
        assert_eq!(card.color, Some(Color::rgba(20, 40, 60, 255)));
        assert_eq!(card.size, Vec2::new(150.0, 90.0));
        assert!(app.doc_mut().unwrap().doc.undo());
        assert_eq!(app.doc().unwrap().doc.sheet(), &before);
    }
}
