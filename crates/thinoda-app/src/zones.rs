//! Zone drafts retain their source identity; applying a frame also moves its content.

use egui::Context;
use thinoda_core::{CanvasObjectId, Color, Rect, TopicId, ZoneId, board::BoardMember};
use thinoda_ui::widgets;

use crate::{app::ThinodaApp, i18n::t};

#[derive(Clone)]
struct ZoneOwner {
    document: String,
    sheet: usize,
    root: TopicId,
    viewport: egui::ViewportId,
}

#[cfg(test)]
#[path = "zones_tests.rs"]
mod tests;

struct ZoneEditor {
    owner: ZoneOwner,
    revision: u64,
    id: Option<ZoneId>,
    name: String,
    roots: Vec<TopicId>,
    objects: Vec<CanvasObjectId>,
    frame: Rect,
    frame_color: Option<Color>,
    background_color: Option<Color>,
    error: Option<String>,
    composing: bool,
}

struct ZoneMembership {
    owner: ZoneOwner,
    revision: u64,
    roots: Vec<TopicId>,
    objects: Vec<CanvasObjectId>,
    target: Option<ZoneId>,
    error: Option<String>,
}

#[derive(Default)]
pub(crate) struct ZoneState {
    manager: Option<ZoneOwner>,
    editor: Option<ZoneEditor>,
    membership: Option<ZoneMembership>,
}

impl ZoneState {
    pub(crate) fn editing(&self) -> bool {
        self.editor.is_some() || self.membership.is_some()
    }
}

fn default_zone_frame_color(dark: bool) -> Color {
    let value = if dark { 170 } else { 85 };
    Color::rgb(value, value, value)
}

fn zone_color_row(
    ui: &mut egui::Ui,
    label: &'static str,
    color: &mut Option<Color>,
    fallback: Color,
    reset_label: &'static str,
) {
    let touch = widgets::touch_activation_allowed(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(t(label));
        let mut value = thinoda_canvas::color32(color.unwrap_or(fallback));
        if ui
            .color_edit_button_srgba(&mut value)
            .labelled_by(label.id)
            .changed()
            && touch
        {
            *color = Some(crate::ui::to_color(value));
        }
        if color.is_some() && ui.button(t(reset_label)).clicked() && touch {
            *color = None;
        }
    });
}

impl ThinodaApp {
    pub(crate) fn selected_zone_members(&self) -> Option<(Vec<TopicId>, Vec<CanvasObjectId>)> {
        let doc = self.doc()?;
        let sheet = doc.doc.sheet();
        if doc.view.selected_object.is_some()
            || (doc.view.selection.is_empty() && doc.view.board_selection.is_empty())
            || doc
                .view
                .selection
                .iter()
                .any(|id| !sheet.floating.contains(id))
        {
            return None;
        }
        let mut roots = doc.view.selection.clone();
        let mut objects = doc.view.board_selection.clone();
        let members: Vec<_> = roots
            .iter()
            .filter_map(|id| sheet.board_member_for_topic(*id))
            .chain(objects.iter().copied().map(BoardMember::Object))
            .collect();
        let expanded = if members.is_empty() {
            Vec::new()
        } else {
            sheet.expand_board_selection(&members).ok()?
        };
        for member in expanded {
            if sheet.is_board_member_locked(member) {
                return None;
            }
            match member {
                BoardMember::Card(id) if !roots.contains(&id) => roots.push(id),
                BoardMember::Object(id) if !objects.contains(&id) => objects.push(id),
                _ => {}
            }
        }
        Some((roots, objects))
    }

    pub(crate) fn zone_selection_has_membership(&self) -> bool {
        self.selected_zone_members()
            .is_some_and(|(roots, objects)| {
                self.doc().is_some_and(|doc| {
                    doc.doc.sheet().zones.iter().any(|zone| {
                        zone.roots.iter().any(|id| roots.contains(id))
                            || zone.objects.iter().any(|id| objects.contains(id))
                    })
                })
            })
    }

    fn zone_membership_draft(&mut self, ctx: &Context) -> Result<ZoneMembership, String> {
        let doc = self
            .doc_mut()
            .ok_or_else(|| t("Not possible here").to_string())?;
        doc.flush_inspector_edit(ctx)?;
        doc.commit_canvas_drafts()?;
        let (roots, objects) = self
            .selected_zone_members()
            .ok_or_else(|| t("Select floating topics or board objects.").to_string())?;
        let doc = self.doc().expect("active document checked above");
        Ok(ZoneMembership {
            owner: ZoneOwner {
                document: doc.key.clone(),
                sheet: doc.doc.active_index(),
                root: doc.doc.sheet().root,
                viewport: ctx.viewport_id(),
            },
            revision: doc.doc.state_id(),
            roots,
            objects,
            target: doc.doc.sheet().zones.first().map(|zone| zone.id),
            error: None,
        })
    }

    pub(crate) fn begin_zone_membership(&mut self, ctx: &Context) {
        match self.zone_membership_draft(ctx) {
            Ok(draft) if draft.target.is_some() => self.zones.membership = Some(draft),
            Ok(_) => {
                self.toasts.warning(t(
                    "No Zones yet. Select floating topics, then create a Zone.",
                ));
            }
            Err(error) => {
                self.toasts.error(error);
            }
        }
    }

    fn apply_zone_membership(&mut self, draft: &ZoneMembership, add: bool) -> Result<(), String> {
        let doc = self
            .docs
            .iter_mut()
            .find(|doc| doc.key == draft.owner.document)
            .ok_or_else(|| t("The document changed. Try again.").to_string())?;
        if doc.doc.state_id() != draft.revision
            || doc.doc.active_index() != draft.owner.sheet
            || doc.doc.sheet().root != draft.owner.root
        {
            return Err(t("The document changed. Try again.").into());
        }
        doc.doc
            .edit_sheet(
                if add {
                    "Add to Zone"
                } else {
                    "Remove from Zone"
                },
                |sheet| {
                    let zones: Vec<_> = sheet
                        .zones
                        .iter()
                        .filter(|zone| {
                            if add {
                                Some(zone.id) == draft.target
                            } else {
                                zone.roots.iter().any(|id| draft.roots.contains(id))
                                    || zone.objects.iter().any(|id| draft.objects.contains(id))
                            }
                        })
                        .cloned()
                        .collect();
                    if add && zones.is_empty() {
                        return Err(thinoda_core::EditError::Invalid(
                            t("The document changed. Try again.").into(),
                        ));
                    }
                    for zone in zones {
                        let mut roots = zone.roots;
                        let mut objects = zone.objects;
                        if add {
                            for id in &draft.roots {
                                if !roots.contains(id) {
                                    roots.push(*id);
                                }
                            }
                            for id in &draft.objects {
                                if !objects.contains(id) {
                                    objects.push(*id);
                                }
                            }
                        } else {
                            roots.retain(|id| !draft.roots.contains(id));
                            objects.retain(|id| !draft.objects.contains(id));
                        }
                        sheet
                            .set_zone_members(zone.id, roots, objects)
                            .map_err(thinoda_core::EditError::Invalid)?;
                    }
                    Ok(())
                },
            )
            .map_err(|error| error.to_string())
    }

    pub(crate) fn remove_from_zones(&mut self, ctx: &Context) {
        let result = self
            .zone_membership_draft(ctx)
            .and_then(|draft| self.apply_zone_membership(&draft, false));
        if let Err(error) = result {
            self.toasts.error(error);
        }
        ctx.request_repaint();
    }

    pub(crate) fn remove_selected_zone(&mut self, ctx: &Context) {
        let Some(doc) = self.doc_mut() else { return };
        let Some(thinoda_canvas::ObjectSelection::Zone(id)) = doc.view.selected_object else {
            return;
        };
        let result = doc
            .flush_inspector_edit(ctx)
            .and_then(|()| doc.commit_canvas_drafts())
            .and_then(|()| {
                doc.doc
                    .edit_sheet("Remove Zone", |sheet| {
                        sheet
                            .remove_zone(id)
                            .map_err(thinoda_core::EditError::Invalid)
                    })
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(()) => doc.view.selected_object = None,
            Err(error) => {
                self.toasts.error(error);
            }
        }
        ctx.request_repaint();
    }

    pub(crate) fn open_zones(&mut self, ctx: &Context) {
        self.zones.manager = self.doc().map(|doc| ZoneOwner {
            document: doc.key.clone(),
            sheet: doc.doc.active_index(),
            root: doc.doc.sheet().root,
            viewport: ctx.viewport_id(),
        });
    }

    pub(crate) fn begin_zone_editor(&mut self, ctx: &Context, id: Option<ZoneId>) {
        let selection = self.selected_zone_members();
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc.flush_inspector_edit(ctx) {
            self.toasts.error(error);
            return;
        }
        if let Err(error) = doc.commit_canvas_drafts() {
            self.toasts.error(error);
            return;
        }
        let (name, roots, objects, frame, frame_color, background_color) = if let Some(id) = id {
            let Some(zone) = doc.doc.sheet().zones.iter().find(|zone| zone.id == id) else {
                self.toasts.warning(t("The document changed. Try again."));
                return;
            };
            (
                zone.name.clone(),
                zone.roots.clone(),
                zone.objects.clone(),
                zone.frame,
                zone.color,
                zone.background_color,
            )
        } else {
            let Some((roots, objects)) = selection else {
                self.toasts
                    .warning(t("Select floating topics or board objects."));
                return;
            };
            let mut bounds = doc.view.topic_group_bounds(&doc.doc, &roots);
            for id in &objects {
                if let Some(object) = doc.doc.sheet().canvas.object(*id) {
                    let rect = object.rotated_bounds();
                    bounds = Some(bounds.map_or(rect, |bounds| bounds.union(&rect)));
                }
            }
            let Some(bounds) = bounds else {
                self.toasts.warning(t("Not possible here"));
                return;
            };
            let mut frame = bounds.expand(24.0);
            frame.size.x = frame.size.x.max(100.0);
            frame.size.y = frame.size.y.max(100.0);
            (
                t("Zone").to_string(),
                roots,
                objects,
                frame,
                Some(thinoda_core::automatic_zone_color(
                    doc.doc.sheet().zones.len(),
                )),
                None,
            )
        };
        self.zones.editor = Some(ZoneEditor {
            owner: ZoneOwner {
                document: doc.key.clone(),
                sheet: doc.doc.active_index(),
                root: doc.doc.sheet().root,
                viewport: ctx.viewport_id(),
            },
            revision: doc.doc.state_id(),
            id,
            name,
            roots,
            objects,
            frame,
            frame_color,
            background_color,
            error: None,
            composing: false,
        });
    }

    fn apply_zone_draft(&mut self, editor: &ZoneEditor) -> Result<(), String> {
        let doc = self
            .docs
            .iter_mut()
            .find(|doc| doc.key == editor.owner.document)
            .ok_or_else(|| t("The document changed. Try again.").to_string())?;
        if doc.doc.state_id() != editor.revision
            || doc.doc.active_index() != editor.owner.sheet
            || doc.doc.sheet().root != editor.owner.root
        {
            return Err(t("The document changed. Try again.").into());
        }
        if let Some(id) = editor.id {
            doc.doc
                .edit_sheet("Edit Zone", |sheet| {
                    let frame = sheet
                        .zones
                        .iter()
                        .find(|zone| zone.id == id)
                        .ok_or_else(|| {
                            thinoda_core::EditError::Invalid(
                                t("The document changed. Try again.").into(),
                            )
                        })?
                        .frame;
                    sheet
                        .rename_zone(id, &editor.name)
                        .map_err(thinoda_core::EditError::Invalid)?;
                    sheet
                        .move_zone(id, editor.frame.min - frame.min)
                        .map_err(thinoda_core::EditError::Invalid)?;
                    sheet
                        .resize_zone(id, editor.frame.size)
                        .map_err(thinoda_core::EditError::Invalid)?;
                    let zone = sheet
                        .zones
                        .iter_mut()
                        .find(|zone| zone.id == id)
                        .ok_or_else(|| {
                            thinoda_core::EditError::Invalid(
                                t("The document changed. Try again.").into(),
                            )
                        })?;
                    zone.color = editor.frame_color;
                    zone.background_color = editor.background_color;
                    Ok(())
                })
                .map_err(|error| error.to_string())
        } else {
            doc.doc
                .edit_sheet("Create Zone", |sheet| {
                    let id = sheet
                        .create_board_frame(&editor.name, editor.frame)
                        .map_err(thinoda_core::EditError::Invalid)?;
                    sheet
                        .set_zone_members(id, editor.roots.clone(), editor.objects.clone())
                        .map_err(thinoda_core::EditError::Invalid)?;
                    let zone = sheet
                        .zones
                        .iter_mut()
                        .find(|zone| zone.id == id)
                        .ok_or_else(|| {
                            thinoda_core::EditError::Invalid(
                                t("The document changed. Try again.").into(),
                            )
                        })?;
                    zone.color = editor.frame_color;
                    zone.background_color = editor.background_color;
                    Ok(())
                })
                .map_err(|error| error.to_string())
        }
    }

    pub(crate) fn zones_window(&mut self, ctx: &Context) {
        let Some(owner) = self
            .zones
            .manager
            .clone()
            .filter(|owner| owner.viewport == ctx.viewport_id())
        else {
            return;
        };
        let Some(index) = self.docs.iter().position(|doc| {
            doc.key == owner.document
                && doc
                    .doc
                    .workbook()
                    .sheets
                    .get(owner.sheet)
                    .is_some_and(|sheet| sheet.root == owner.root)
        }) else {
            self.zones.manager = None;
            return;
        };
        let mut open = true;
        let mut edit = None;
        let mut create = false;
        let mut reorder = None;
        let mut remove = None;
        let blocked = self.any_modal_open();
        let touch = widgets::touch_activation_allowed(ctx);
        let revision = self.docs[index].doc.state_id();
        let labels = crate::i18n::dialog_labels();
        let _ = thinoda_ui::Dialog::new(egui::Id::unique(("zones", &owner.document, owner.sheet)), t("Zones"))
            .icon(thinoda_ui::icons::ZONES)
            .labels(&labels)
            .open(&mut open)
            .default_width(440.0)
            .default_height(360.0)
            .scroll(false)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!blocked, |ui| {
                    let doc = &self.docs[index];
                    let sheet = &doc.doc.workbook().sheets[owner.sheet];
                    ui.label(&sheet.title);
                    ui.label(t("Zones group floating topics and board objects. Removing a Zone keeps its contents."));
                    let can_create = self.active == index && doc.doc.active_index() == owner.sheet
                        && self.selected_zone_members().is_some();
                    create = ui.add_enabled(can_create, egui::Button::new(t("Create Zone")))
                        .on_disabled_hover_text(t("Select floating topics or board objects."))
                        .clicked() && touch;
                    egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-180.0).max(80.0))
                        .show(ui, |ui| {
                            if sheet.zones.is_empty() { ui.label(t("No Zones yet. Select floating topics, then create a Zone.")); }
                            for (position, zone) in sheet.zones.iter().enumerate() {
                                ui.push_id(zone.id, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.button(&zone.name).clicked() && touch { edit = Some(zone.id); }
                                        if ui.add_enabled(position > 0, egui::Button::new(t("Move Up"))).clicked() && touch {
                                            reorder = Some((zone.id, position-1));
                                        }
                                        if ui.add_enabled(position+1 < sheet.zones.len(), egui::Button::new(t("Move Down"))).clicked() && touch {
                                            reorder = Some((zone.id, position+1));
                                        }
                                        if ui.button(t("Remove Zone")).clicked() && touch { remove = Some(zone.id); }
                                    });
                                });
                            }
                        });
                });
            });
        if !open {
            self.zones.manager = None;
        }
        if create || edit.is_some() {
            let active = self.active;
            self.active = index;
            if let Err(error) = self.docs[index].switch_sheet(owner.sheet) {
                self.toasts.error(error);
            } else {
                self.begin_zone_editor(ctx, edit);
            }
            self.active = active;
        }
        if reorder.is_some() || remove.is_some() {
            let doc = &mut self.docs[index];
            let result = doc.flush_inspector_edit(ctx).and_then(|()| {
                doc.commit_canvas_drafts()?;
                if doc.doc.state_id() != revision {
                    return Err(t("The document changed. Try again.").into());
                }
                doc.doc
                    .edit_book("Manage Zones", |book| {
                        let sheet = &mut book.sheets[owner.sheet];
                        if let Some((id, position)) = reorder {
                            sheet
                                .reorder_zone(id, position)
                                .map_err(thinoda_core::EditError::Invalid)?;
                        }
                        if let Some(id) = remove {
                            sheet
                                .remove_zone(id)
                                .map_err(thinoda_core::EditError::Invalid)?;
                        }
                        Ok(())
                    })
                    .map_err(|error| error.to_string())
            });
            if let Err(error) = result {
                self.toasts.error(error);
            }
            ctx.request_repaint();
        }
    }

    pub(crate) fn zone_dialog(&mut self, ctx: &Context) {
        self.zone_membership_dialog(ctx);
        if self
            .zones
            .editor
            .as_ref()
            .is_some_and(|editor| editor.owner.viewport != ctx.viewport_id())
        {
            return;
        }
        let Some(mut editor) = self.zones.editor.take() else {
            return;
        };
        let ime = ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)))
        });
        let focus = ctx
            .memory(|memory| memory.focused())
            .unwrap_or(egui::Id::unique("zone-ime"));
        thinoda_canvas::protect_ime_candidate_keys(ctx, focus, &mut editor.composing);
        let touch = widgets::touch_activation_allowed(ctx);
        let mut apply = false;
        let mut cancel = false;
        let title = if editor.id.is_some() {
            "Edit Zone"
        } else {
            "Create Zone"
        };
        let labels = crate::i18n::dialog_labels();
        let response = thinoda_ui::Dialog::new(egui::Id::unique("zone-editor"), t(title))
            .icon(thinoda_ui::icons::ZONES)
            .labels(&labels)
            .modal(true)
            .close_on_backdrop(true)
            .default_width(420.0)
            .default_height(500.0)
            .min_size(egui::vec2(300.0, 240.0))
            .scroll(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 150.0).max(40.0))
                    .show(ui, |ui| {
                        let label = ui.label(t("Name"));
                        ui.add(egui::TextEdit::singleline(&mut editor.name))
                            .labelled_by(label.id);
                        let owner = Some(egui::Id::unique((
                            &editor.owner.document,
                            editor.owner.sheet,
                            editor.revision,
                            editor.owner.viewport,
                            editor.id,
                        )));
                        for (salt, label, value, minimum) in [
                            ("zone-x", "X", &mut editor.frame.min.x, -f32::MAX),
                            ("zone-y", "Y", &mut editor.frame.min.y, -f32::MAX),
                            ("zone-width", "Width", &mut editor.frame.size.x, 100.0),
                            ("zone-height", "Height", &mut editor.frame.size.y, 100.0),
                        ] {
                            let label = ui.label(t(label));
                            let mut draft = *value;
                            let (_, commit) = crate::ui::touch_numeric_value(
                                ui,
                                &mut draft,
                                salt,
                                owner,
                                |ui, value| {
                                    ui.add(egui::DragValue::new(value).range(minimum..=f32::MAX))
                                        .labelled_by(label.id)
                                },
                            );
                            if commit {
                                *value = draft;
                            }
                        }
                        let default_frame = default_zone_frame_color(ui.visuals().dark_mode);
                        let background_fallback = editor.frame_color.unwrap_or(default_frame);
                        zone_color_row(
                            ui,
                            "Frame color",
                            &mut editor.frame_color,
                            default_frame,
                            "Use theme default",
                        );
                        zone_color_row(
                            ui,
                            "Background color",
                            &mut editor.background_color,
                            background_fallback,
                            "Use frame color",
                        );
                        ui.label(t(
                            "Moving a Zone moves its topics. Minimum size is 100 × 100.",
                        ));
                        if let Some(error) = &editor.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                    });
                ui.horizontal_wrapped(|ui| {
                    apply = ui.button(t("Apply")).clicked() && touch;
                    cancel = ui.button(t("Cancel")).clicked() && touch;
                });
            });
        if apply && !ime && !editor.composing {
            match self.apply_zone_draft(&editor) {
                Ok(()) => {
                    ctx.request_repaint();
                    return;
                }
                Err(error) => {
                    log::warn!("Zone draft: {error}");
                    editor.error = Some(error);
                }
            }
        }
        if !(cancel || response.close_requested) || ime || editor.composing {
            self.zones.editor = Some(editor);
        }
    }

    pub(crate) fn return_zone_viewport(&mut self, viewport: egui::ViewportId) {
        if let Some(draft) = &mut self.zones.membership
            && draft.owner.viewport == viewport
        {
            draft.owner.viewport = egui::ViewportId::ROOT;
        }
        if let Some(editor) = &mut self.zones.editor
            && editor.owner.viewport == viewport
        {
            editor.owner.viewport = egui::ViewportId::ROOT;
        }

        if let Some(manager) = &mut self.zones.manager
            && manager.viewport == viewport
        {
            manager.viewport = egui::ViewportId::ROOT;
        }
    }

    fn zone_membership_dialog(&mut self, ctx: &Context) {
        if self
            .zones
            .membership
            .as_ref()
            .is_none_or(|draft| draft.owner.viewport != ctx.viewport_id())
        {
            return;
        }
        let Some(mut draft) = self.zones.membership.take() else {
            return;
        };
        let zones: Vec<_> = self
            .docs
            .iter()
            .find(|doc| doc.key == draft.owner.document)
            .and_then(|doc| doc.doc.workbook().sheets.get(draft.owner.sheet))
            .map(|sheet| {
                sheet
                    .zones
                    .iter()
                    .map(|zone| (zone.id, zone.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let mut apply = false;
        let mut cancel = false;
        let touch = widgets::touch_activation_allowed(ctx);
        let labels = crate::i18n::dialog_labels();
        let response =
            thinoda_ui::Dialog::new(egui::Id::unique("zone-membership"), t("Add to Zone…"))
                .icon(thinoda_ui::icons::ZONES)
                .labels(&labels)
                .modal(true)
                .close_on_backdrop(true)
                .default_width(420.0)
                .default_height(320.0)
                .scroll(false)
                .show(ctx, |ui| {
                    ui.label(t(
                        "Choose a Zone. Items stay in place; existing memberships are transferred.",
                    ));
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 230.0).max(40.0))
                        .show(ui, |ui| {
                            for (id, name) in &zones {
                                if ui
                                    .selectable_label(draft.target == Some(*id), name)
                                    .clicked()
                                    && touch
                                {
                                    draft.target = Some(*id);
                                }
                            }
                        });
                    if let Some(error) = &draft.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    ui.horizontal_wrapped(|ui| {
                        apply = ui
                            .add_enabled(draft.target.is_some(), egui::Button::new(t("Apply")))
                            .clicked()
                            && touch;
                        cancel = ui.button(t("Cancel")).clicked() && touch;
                    });
                });
        if apply {
            match self.apply_zone_membership(&draft, true) {
                Ok(()) => {
                    ctx.request_repaint();
                    return;
                }
                Err(error) => {
                    log::warn!("Zone membership: {error}");
                    draft.error = Some(error);
                }
            }
        }
        if !cancel && !response.close_requested {
            self.zones.membership = Some(draft);
        }
    }

    pub(crate) fn zone_properties(&mut self, ui: &mut egui::Ui, id: ZoneId) {
        if let Some(zone) = self
            .doc()
            .and_then(|doc| doc.doc.sheet().zones.iter().find(|zone| zone.id == id))
        {
            ui.heading(&zone.name);
            ui.label(format!(
                "{:.0} × {:.0}",
                zone.frame.size.x, zone.frame.size.y
            ));
            if ui.button(t("Edit Zone")).clicked() && widgets::touch_activation_allowed(ui.ctx()) {
                self.begin_zone_editor(ui.ctx(), Some(id));
            }
        }
    }
}
