//! Compact board Layers panel shared by native and web views.

use egui::{Context, Rect, TextEdit, pos2};
use thinoda_core::{BoardLayer, BoardLayerId};
use thinoda_ui::{icons, widgets};

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};

const PANEL_WIDTH: f32 = 300.0;
const PANEL_TOP_INSET: f32 = crate::board_ui::CONTEXT_STRIP_HEIGHT + 16.0;

#[derive(Clone, Debug, PartialEq)]
enum LayerAction {
    Close,
    Add,
    Rename(BoardLayerId, String),
    Visible(BoardLayerId, bool),
    Locked(BoardLayerId, bool),
    Reorder(BoardLayerId, usize),
    Delete(BoardLayerId),
    MoveSelection(Option<BoardLayerId>),
}

fn named(label: &'static str, layer: &str) -> String {
    format!("{}: {layer}", t(label))
}

/// The next free "Layer N" name.
fn next_layer_name(layers: &[BoardLayer]) -> String {
    (layers.len() + 1..)
        .map(|n| format!("{} {n}", t("Layer")))
        .find(|name| !layers.iter().any(|layer| &layer.name == name))
        .unwrap_or_else(|| t("Layer").to_owned())
}

impl ThinodaApp {
    pub(crate) fn board_layers_panel(&mut self, ctx: &Context, canvas: Rect) {
        let Some(doc) = self.doc() else { return };
        if !doc.show_layers || !self.command_available(&Cmd::BoardLayers) {
            return;
        }
        let key = doc.key.clone();
        let layers = doc.doc.sheet().canvas.layers.clone();
        let has_selection = !doc.view.board_selection.is_empty();
        let touch = widgets::touch_activation_allowed(ctx);
        let width = PANEL_WIDTH.min((canvas.width() - 40.0).max(0.0));
        let mut action = None;
        egui::Area::new(egui::Id::unique(("thinoda_board_layers", &key)))
            .order(egui::Order::Middle)
            .fixed_pos(pos2(
                canvas.right() - width - 28.0,
                canvas.top() + PANEL_TOP_INSET,
            ))
            .constrain_to(canvas)
            .movable(false)
            .show(ctx, |ui| {
                widgets::card(ui).show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.strong(t("Layers"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(ui, icons::CLOSE, t("Close"), false).clicked()
                                && touch
                            {
                                action = Some(LayerAction::Close);
                            }
                            if widgets::icon_button(ui, icons::ADD, t("Add layer"), false).clicked()
                                && touch
                            {
                                action = Some(LayerAction::Add);
                            }
                        });
                    });
                    if layers.is_empty() {
                        widgets::caption(
                            ui,
                            t("Only the base layer exists. Add a layer to hide or lock board objects together."),
                        );
                    }
                    let max_height = (canvas.height() - PANEL_TOP_INSET - 96.0).max(80.0);
                    egui::ScrollArea::vertical()
                        .id_salt("board_layers_scroll")
                        .max_height(max_height)
                        .show(ui, |ui| {
                            let count = layers.len();
                            for (index, layer) in layers.iter().enumerate().rev() {
                                ui.push_id(layer.id, |ui| {
                                    if let Some(row) =
                                        layer_row(ui, layer, index, count, has_selection, touch)
                                    {
                                        action = Some(row);
                                    }
                                });
                            }
                            ui.horizontal(|ui| {
                                ui.add_space(2.0 * (thinoda_ui::tokens::Tokens::ICON_BUTTON + ui.spacing().item_spacing.x));
                                ui.label(t("Base layer"));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add_enabled_ui(has_selection, |ui| {
                                                widgets::icon_button(
                                                    ui,
                                                    icons::IMPORT,
                                                    &named("Move selection here", t("Base layer")),
                                                    false,
                                                )
                                            })
                                            .inner
                                            .clicked()
                                            && touch
                                        {
                                            action = Some(LayerAction::MoveSelection(None));
                                        }
                                    },
                                );
                            });
                        });
                    crate::ui::reveal_changed_focus(ui);
                });
            });
        if let Some(action) = action {
            self.apply_layer_action(action, &layers);
            ctx.request_repaint();
        }
    }

    fn apply_layer_action(&mut self, action: LayerAction, layers: &[BoardLayer]) {
        if action == LayerAction::Close {
            if let Some(doc) = self.doc_mut() {
                doc.show_layers = false;
            }
            return;
        }
        if !self.finish_canvas_text_edit() {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc.ensure_no_pending_ink() {
            self.toasts.warning(error);
            return;
        }
        let name = |id: BoardLayerId| {
            layers
                .iter()
                .find(|layer| layer.id == id)
                .map_or_else(String::new, |layer| layer.name.clone())
        };
        let result = match action {
            LayerAction::Close => unreachable!(),
            LayerAction::Add => {
                let layer_name = next_layer_name(layers);
                doc.doc
                    .add_board_layer(&layer_name)
                    .map(|_| named("Layer added", &layer_name))
            }
            LayerAction::Rename(id, new_name) => doc
                .doc
                .rename_board_layer(id, &new_name)
                .map(|()| named("Layer renamed", &new_name)),
            LayerAction::Visible(id, visible) => {
                let result = doc.doc.set_board_layer_visible(id, visible);
                doc.view.prune_selection(doc.doc.sheet());
                result.map(|()| {
                    named(
                        if visible {
                            "Layer shown"
                        } else {
                            "Layer hidden"
                        },
                        &name(id),
                    )
                })
            }
            LayerAction::Locked(id, locked) => {
                doc.doc.set_board_layer_locked(id, locked).map(|()| {
                    named(
                        if locked {
                            "Layer locked"
                        } else {
                            "Layer unlocked"
                        },
                        &name(id),
                    )
                })
            }
            LayerAction::Reorder(id, index) => doc
                .doc
                .reorder_board_layer(id, index)
                .map(|()| named("Layer moved", &name(id))),
            LayerAction::Delete(id) => doc
                .doc
                .remove_board_layer(id)
                .map(|()| named("Layer deleted", &name(id))),
            LayerAction::MoveSelection(target) => {
                let selection = doc.view.board_selection.clone();
                let target_name = target.map_or_else(|| t("Base layer").to_owned(), name);
                doc.doc
                    .move_canvas_objects_to_layer(&selection, target)
                    .map(|()| named("Moved selection to layer", &target_name))
            }
        };
        match result {
            Ok(message) => {
                self.toasts.info(message);
            }
            Err(error) => {
                self.toasts
                    .error(format!("{}: {error}", t("Not possible here")));
            }
        }
    }
}

fn layer_row(
    ui: &mut egui::Ui,
    layer: &BoardLayer,
    index: usize,
    count: usize,
    has_selection: bool,
    touch: bool,
) -> Option<LayerAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        let visibility = if layer.visible {
            "Hide layer"
        } else {
            "Show layer"
        };
        if widgets::icon_button(
            ui,
            icons::VISIBLE,
            &named(visibility, &layer.name),
            layer.visible,
        )
        .clicked()
            && touch
        {
            action = Some(LayerAction::Visible(layer.id, !layer.visible));
        }
        let (lock_icon, lock_label) = if layer.locked {
            (icons::PIN, "Unlock layer")
        } else {
            (icons::UNPIN, "Lock layer")
        };
        if widgets::icon_button(ui, lock_icon, &named(lock_label, &layer.name), layer.locked)
            .clicked()
            && touch
        {
            action = Some(LayerAction::Locked(layer.id, !layer.locked));
        }
        let buttons = 4.0 * (thinoda_ui::tokens::Tokens::ICON_BUTTON + ui.spacing().item_spacing.x);
        let name_width = (ui.available_width() - buttons).max(40.0);
        let edit_id = ui.make_persistent_id("name");
        let draft_id = edit_id.with("draft");
        let mut draft = ui
            .data(|data| data.get_temp::<String>(draft_id))
            .unwrap_or_else(|| layer.name.clone());
        let response = ui.add(
            TextEdit::singleline(&mut draft)
                .id(edit_id)
                .desired_width(name_width)
                .char_limit(thinoda_core::board_layers::MAX_BOARD_LAYER_NAME_CHARS)
                .hint_text(t("Layer name")),
        );
        let focused = response.has_focus();
        if response.lost_focus() {
            let cancelled = ui.input(|input| input.key_pressed(egui::Key::Escape));
            let name = draft.trim();
            if !cancelled && !name.is_empty() && name != layer.name {
                action = Some(LayerAction::Rename(layer.id, name.to_owned()));
            }
            ui.data_mut(|data| data.remove::<String>(draft_id));
        } else if focused {
            ui.data_mut(|data| data.insert_temp(draft_id, draft));
        }
        let movable = layer.visible && !layer.locked && has_selection;
        let move_here = ui
            .add_enabled_ui(movable, |ui| {
                widgets::icon_button(
                    ui,
                    icons::IMPORT,
                    &named("Move selection here", &layer.name),
                    false,
                )
            })
            .inner;
        if move_here.clicked() && touch {
            action = Some(LayerAction::MoveSelection(Some(layer.id)));
        }
        let up = ui
            .add_enabled_ui(index + 1 < count, |ui| {
                widgets::icon_button(
                    ui,
                    icons::MOVE_UP,
                    &named("Move layer up", &layer.name),
                    false,
                )
            })
            .inner;
        if up.clicked() && touch {
            action = Some(LayerAction::Reorder(layer.id, index + 1));
        }
        let down = ui
            .add_enabled_ui(index > 0, |ui| {
                widgets::icon_button(
                    ui,
                    icons::MOVE_DOWN,
                    &named("Move layer down", &layer.name),
                    false,
                )
            })
            .inner;
        if down.clicked() && touch {
            action = Some(LayerAction::Reorder(layer.id, index.saturating_sub(1)));
        }
        let delete = ui
            .add_enabled_ui(!layer.locked, |ui| {
                widgets::icon_button(ui, icons::TRASH, &named("Delete layer", &layer.name), false)
            })
            .inner;
        if delete.clicked() && touch {
            action = Some(LayerAction::Delete(layer.id));
        }
    });
    action
}

#[cfg(test)]
mod tests;
