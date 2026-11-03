//! Board tools and accessible labels shared by native and web views.

use egui::{Color32, Context, Rect, Ui, vec2};
use thinoda_canvas::{BoardTool, MapView};
use thinoda_core::{
    CanvasObject, CanvasObjectKind, Color, ConnectorRoute, InkTool, ObjectStyle, ShapeKind,
    board::{BoardMember, MAX_BOARD_COORDINATE, MIN_CARD_SIZE},
};
use thinoda_ui::{
    icons,
    ribbon::{self, CommandIcon, Detail},
    widgets,
};

use crate::{
    app::{Cmd, ThinodaApp},
    command_icons::board_tool_glyph,
    i18n::{self, t},
};

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
                },
                CanvasObjectKind::Connector(connector),
            ) => {
                object.style = style;
                connector.color = color;
                connector.width = width;
                connector.route = route;
                connector.arrow_start = arrow_start;
                connector.arrow_end = arrow_end;
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
    pub(crate) fn board_toolbar(&mut self, ctx: &Context, canvas: Rect) -> Option<Rect> {
        let doc = self.doc()?;
        let tool = doc.view.board_tool;
        let show_minimap = doc.view.show_minimap;
        let mut ink_settings = doc.view.ink_settings;
        let collapsed_id = egui::Id::unique(("board_toolbar_collapsed", &doc.key));
        let auto_color_id = egui::Id::unique(("board_ink_auto_color", &doc.key));
        let last_tool_id = egui::Id::unique(("board_toolbar_last_tool", &doc.key));
        let reveal_context = matches!(
            tool,
            BoardTool::Pen
                | BoardTool::Highlighter
                | BoardTool::ObjectEraser
                | BoardTool::PixelEraser
        ) && ctx.data(|data| data.get_temp::<BoardTool>(last_tool_id))
            != Some(tool);
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
        let content_width = (canvas.width() - 36.0).max(0.0);
        let toolbar_width = if collapsed {
            content_width.min(34.0)
        } else {
            content_width.min(200.0)
        };
        let toolbar_left = canvas.left() + 12.0;
        let toolbar_right = toolbar_left + toolbar_width + 16.0;
        let editing_toolbar = ctx
            .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_editing_toolbar")))
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
                        if matches!(tool, BoardTool::Pen | BoardTool::Highlighter) {
                            let color_control = board_tool_popup(
                                ui,
                                icons::FORMAT.into(),
                                "Ink color",
                                collapsed,
                                false,
                                row_height,
                                |ui| {
                                    ui.checkbox(&mut auto_color, t("Auto"));
                                    if !auto_color {
                                        let mut color = Color32::from_rgba_unmultiplied(
                                            ink_settings.color.r,
                                            ink_settings.color.g,
                                            ink_settings.color.b,
                                            ink_settings.color.a,
                                        );
                                        if ui
                                            .color_edit_button_srgba(&mut color)
                                            .accessible_name(t("Ink color"))
                                            .changed()
                                        {
                                            let [r, g, b, a] = color.to_srgba_unmultiplied();
                                            ink_settings.color = Color::rgba(r, g, b, a);
                                        }
                                    }
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
                                    ui.add(egui::DragValue::new(&mut ink_settings.width).range(
                                        thinoda_core::board::MIN_STROKE_WIDTH
                                            ..=thinoda_core::board::MAX_STROKE_WIDTH,
                                    ))
                                    .accessible_name(t("Ink width"));
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
                                let id = ui.make_persistent_id("board_shape_search");
                                let mut query =
                                    ui.data(|data| data.get_temp::<String>(id).unwrap_or_default());
                                ui.add(
                                    egui::TextEdit::singleline(&mut query)
                                        .hint_text(t("Search shapes"))
                                        .desired_width(140.0),
                                )
                                .accessible_name(t("Search shapes"));
                                ui.data_mut(|data| data.insert_temp(id, query.clone()));
                                let query = query.to_lowercase();
                                egui::ScrollArea::vertical()
                                    .max_height((canvas.height() * 0.6).min(280.0))
                                    .show(ui, |ui| {
                                        for shape in ShapeKind::ALL {
                                            let name = t(shape_name(shape));
                                            if !query.is_empty()
                                                && !name.to_lowercase().contains(&query)
                                            {
                                                continue;
                                            }
                                            if ui
                                                .selectable_label(
                                                    tool == BoardTool::Shape(shape),
                                                    name,
                                                )
                                                .clicked()
                                            {
                                                command =
                                                    Some(Cmd::BoardTool(BoardTool::Shape(shape)));
                                                ui.close_kind(egui::UiKind::Menu);
                                            }
                                        }
                                    });
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
                    });
                });
            });
        ctx.data_mut(|data| {
            data.insert_temp(collapsed_id, collapsed);
            data.insert_temp(auto_color_id, auto_color);
            data.insert_temp(last_tool_id, tool);
        });
        if let Some(doc) = self.doc_mut() {
            doc.view.ink_settings = ink_settings;
        }
        if let Some(command) = command {
            self.exec(ctx, command);
        }
        Some(toolbar.response.rect)
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
        let mut edited = original.clone();
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
                    ui.checkbox(&mut connector.arrow_start, t("Arrow at start"));
                    ui.checkbox(&mut connector.arrow_end, t("Arrow at end"));
                    optional_color(ui, "Stroke", &mut connector.color, Color::BLACK);
                    width_field(ui, &mut connector.width);
                }
                CanvasObjectKind::Ink(ink) => {
                    color_field(ui, "Stroke", &mut ink.color);
                    width_field(ui, &mut ink.width);
                }
            }
        });
        if edited != original {
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
                                current.color = value.color;
                                current.width = value.width;
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
    }
    response
}

fn route_name(route: ConnectorRoute) -> &'static str {
    match route {
        ConnectorRoute::Straight => "Straight",
        ConnectorRoute::Elbow => "Elbow",
        ConnectorRoute::Curve => "Curve",
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
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
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
        app.board_toolbar(ui.ctx(), ui.max_rect());
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

    #[test]
    fn maps_and_board_documents_start_collapsed_and_can_expand() {
        let mut legacy = Harness::builder().build_ui_state(
            |ui, app: &mut ThinodaApp| {
                app.board_toolbar(ui.ctx(), ui.max_rect());
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
                app.board_toolbar(ui.ctx(), ui.max_rect());
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
        harness.get_by_label("Shape library").click();
        harness.run_steps(2);
        harness
            .get_by_role_and_label(egui::Role::TextInput, "Search shapes")
            .click();
        harness.run_steps(2);
        harness
            .get_by_role_and_label(egui::Role::TextInput, "Search shapes")
            .type_text("Pentagon");
        harness.run_steps(2);
        assert!(harness.query_by_label("Triangle").is_none());
        harness.get_by_label("Pentagon").click();
        harness.run_steps(2);
        assert_eq!(
            harness.state().doc().unwrap().view.board_tool,
            BoardTool::Shape(ShapeKind::Pentagon)
        );
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
        assert_eq!(connector.width, 4.0);
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
