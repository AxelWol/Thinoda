//! Zone drafts retain their source identity; applying a frame also moves its content.

use egui::Context;
use thinoda_core::{Rect, TopicId, ZoneId};
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
    frame: Rect,
    error: Option<String>,
    composing: bool,
}

#[derive(Default)]
pub(crate) struct ZoneState {
    manager: Option<ZoneOwner>,
    editor: Option<ZoneEditor>,
}

impl ZoneState {
    pub(crate) fn editing(&self) -> bool {
        self.editor.is_some()
    }
}

impl ThinodaApp {
    pub(crate) fn open_zones(&mut self, ctx: &Context) {
        self.zones.manager = self.doc().map(|doc| ZoneOwner {
            document: doc.key.clone(),
            sheet: doc.doc.active_index(),
            root: doc.doc.sheet().root,
            viewport: ctx.viewport_id(),
        });
    }

    pub(crate) fn begin_zone_editor(&mut self, ctx: &Context, id: Option<ZoneId>) {
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc.flush_inspector_edit(ctx) {
            self.toasts.error(error);
            return;
        }
        doc.view.commit_edit(&mut doc.doc);
        let (name, roots, frame) = if let Some(id) = id {
            let Some(zone) = doc.doc.sheet().zones.iter().find(|zone| zone.id == id) else {
                self.toasts.warning(t("The document changed. Try again."));
                return;
            };
            (zone.name.clone(), zone.roots.clone(), zone.frame)
        } else {
            let roots = doc.view.selection.clone();
            if roots.is_empty()
                || roots
                    .iter()
                    .any(|id| !doc.doc.sheet().floating.contains(id))
            {
                self.toasts
                    .warning(t("Select floating topics to create a Zone."));
                return;
            }
            let Some(bounds) = doc.view.topic_group_bounds(&doc.doc, &roots) else {
                self.toasts.warning(t("Not possible here"));
                return;
            };
            let mut frame = bounds.expand(24.0);
            frame.size.x = frame.size.x.max(100.0);
            frame.size.y = frame.size.y.max(100.0);
            (t("Zone").to_string(), roots, frame)
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
            frame,
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
                    Ok(())
                })
                .map_err(|error| error.to_string())
        } else {
            doc.doc
                .create_zone(&editor.name, editor.roots.clone(), editor.frame)
                .map(|_| ())
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
        egui::Window::new(t("Zones")).id(egui::Id::unique(("zones", &owner.document, owner.sheet)))
            .open(&mut open).default_size([440.0, 360.0])
            .max_size(ctx.content_rect().size() - egui::vec2(24.0, 24.0))
            .show(ctx, |ui| {
                ui.add_enabled_ui(!blocked, |ui| {
                    let doc = &self.docs[index];
                    let sheet = &doc.doc.workbook().sheets[owner.sheet];
                    ui.label(&sheet.title);
                    ui.label(t("Zones group floating topics and their descendants. Removing a Zone keeps its topics."));
                    let can_create = doc.doc.active_index() == owner.sheet
                        && !doc.view.selection.is_empty()
                        && doc.view.selection.iter().all(|id| sheet.floating.contains(id));
                    create = ui.add_enabled(can_create, egui::Button::new(t("Create Zone")))
                        .on_disabled_hover_text(t("Select floating topics to create a Zone."))
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
            if self.docs[index].doc.active_index() != owner.sheet {
                self.docs[index].view.reset();
                self.docs[index].doc.set_active(owner.sheet);
            }
            self.begin_zone_editor(ctx, edit);
            self.active = active;
        }
        if reorder.is_some() || remove.is_some() {
            let doc = &mut self.docs[index];
            let result = doc.flush_inspector_edit(ctx).and_then(|()| {
                doc.view.commit_edit(&mut doc.doc);
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
        let response = egui::Modal::new(egui::Id::unique("zone-editor")).show(ctx, |ui| {
            ui.set_width(420.0_f32.min((ctx.content_rect().width() - 32.0).max(1.0)));
            ui.heading(t(if editor.id.is_some() {
                "Edit Zone"
            } else {
                "Create Zone"
            }));
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
        if !(cancel || response.should_close()) || ime || editor.composing {
            self.zones.editor = Some(editor);
        }
    }

    pub(crate) fn return_zone_viewport(&mut self, viewport: egui::ViewportId) {
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
