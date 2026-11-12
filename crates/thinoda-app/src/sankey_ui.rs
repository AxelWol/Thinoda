//! Data inspector for semantic flow graphs. All edits use the ordinary document
//! transaction, so over-cap edits are rejected atomically and Undo restores all.
use crate::{app::ThinodaApp, i18n::t};
use egui::Ui;
use thinoda_core::{
    Sheet, TopicId,
    board::{Band, BoardMember, CanvasConnector, CanvasObject, CanvasObjectKind, Endpoint},
    sankey::{self, Flow, SankeyMode},
};

#[cfg(test)]
#[path = "sankey_ui_tests.rs"]
mod tests;

#[derive(Clone)]
struct NumberDraft {
    value: f64,
    base: f64,
    error: Option<String>,
}

fn number_edit(
    ui: &mut Ui,
    key: egui::Id,
    label: egui::Id,
    stored: f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    commits: &mut Vec<egui::Id>,
) -> Option<f64> {
    let mut draft = ui
        .ctx()
        .data_mut(|d| d.get_temp::<NumberDraft>(key))
        .unwrap_or(NumberDraft {
            value: stored,
            base: stored,
            error: None,
        });
    if draft.base != stored && draft.error.is_none() {
        draft.value = stored;
        draft.base = stored;
    }
    let response = ui
        .push_id(key, |ui| {
            ui.add(
                egui::DragValue::new(&mut draft.value)
                    .suffix(if *range.end() == 100.0 { "%" } else { "" })
                    .range(range)
                    .max_decimals(decimals),
            )
        })
        .inner
        .labelled_by(label);
    if response.lost_focus() {
        // Completing a number belongs to this editor, not the canvas's
        // Enter-to-insert-topic shortcut later in the same frame.
        ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    }
    let mut commit = response.drag_stopped()
        || response.lost_focus()
        || (response.changed() && !response.dragged() && !response.has_focus());
    if let Some(error) = &draft.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
        ui.horizontal(|ui| {
            if ui.button(t("Apply")).clicked() {
                commit = true;
            }
            if ui.button(t("Cancel")).clicked() {
                draft.value = stored;
                draft.error = None;
                commit = false;
                response.surrender_focus();
            }
        });
    }
    let result = (commit && draft.value != stored).then_some(draft.value);
    ui.ctx().data_mut(|d| d.insert_temp(key, draft));
    if result.is_some() {
        commits.push(key);
    }
    result
}

fn node_locked(sheet: &Sheet, id: TopicId) -> bool {
    sheet.is_board_member_locked(BoardMember::Card(id))
}

fn node_label(sheet: &Sheet, id: TopicId) -> String {
    let name = sheet
        .topic(id)
        .map(|topic| topic.title.as_str())
        .unwrap_or("?");
    if let Some(settings) = &sheet.sankey
        && settings.mode == SankeyMode::Alluvial
    {
        let stage = settings
            .stages
            .get(&id)
            .map(u32::to_string)
            .unwrap_or_else(|| t("Unassigned").to_owned());
        format!("{name} · {} {stage}", t("Stage"))
    } else {
        name.to_owned()
    }
}

/// Alluvial stage columns with whole-diagram stage actions. Per-node stage
/// numbers remain editable below; these actions only fill or renumber.
fn stage_overview(
    ui: &mut Ui,
    original: &Sheet,
    sheet: &mut Sheet,
    error: &mut Option<String>,
    focus_node: &mut Option<TopicId>,
) {
    egui::CollapsingHeader::new(t("Stages"))
        .default_open(true)
        .show(ui, |ui| {
            let summary = sankey::stage_summary(original);
            if summary.stages.is_empty() {
                ui.small(t("No stages assigned yet."));
            }
            let name = |id: TopicId| {
                original
                    .topic(id)
                    .map(|topic| topic.title.clone())
                    .unwrap_or_default()
            };
            let mut row = |ui: &mut Ui, heading: String, ids: &[TopicId]| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!("{heading} ({})", ids.len()));
                    for &id in ids {
                        if ui.small_button(name(id)).clicked() {
                            *focus_node = Some(id);
                        }
                    }
                });
            };
            for (stage, ids) in &summary.stages {
                row(ui, format!("{} {stage}", t("Stage")), ids);
            }
            if ui
                .button(t("Compact stage numbers"))
                .on_hover_text(t("Number stages consecutively from 0 without gaps."))
                .clicked()
                && let Err(e) = sankey::compact_stages(sheet)
            {
                *error = Some(e);
            }
        });
}

/// New alluvial links may reach unstaged topics; give them the adjacent stage.
fn stage_new_alluvial_link(sheet: &mut Sheet, error: &mut Option<String>) {
    if error.is_none()
        && sheet
            .sankey
            .as_ref()
            .is_some_and(|s| s.mode == SankeyMode::Alluvial)
        && let Err(e) = sankey::assign_missing_stages(sheet)
    {
        *error = Some(e);
    }
}

impl ThinodaApp {
    pub(crate) fn sankey_properties(&mut self, ui: &mut Ui) {
        let Some(doc) = self.doc() else { return };
        let owner = (
            doc.doc.instance_id(),
            doc.doc.active_index(),
            doc.doc.sheet().root,
        );
        if doc.doc.sheet().fim.is_some() {
            return;
        }
        if !doc.doc.sheet().has_sankey() {
            let mut enable = false;
            ui.push_id(("sankey-data",owner),|ui| {
                egui::CollapsingHeader::new(t("Weights")).show(ui,|ui| {
                    enable=ui.button(t("Enable Sankey flows")).clicked();
                    ui.label(t("Connect topic names, notes and hyperlinks without changing mind-map parents."));
                });
            });
            if enable
                && let Some(doc) = self.doc_mut()
                && let Err(e) = doc.doc.edit_sheet("Enable Sankey", |sheet| {
                    sheet.sankey = Some(Default::default());
                    Ok(())
                })
            {
                self.toasts.error(e.to_string());
            }
            return;
        }
        let original = doc.doc.sheet().clone();
        let primary = doc.view.primary();
        let selected = doc.view.board_selection.last().copied();
        if original.fim.is_some() {
            return;
        }

        let mut sheet = original.clone();
        let mut error = None;
        let mut removed = Vec::new();
        let mut focus_link = None;
        let mut focus_node = None;
        let mut commits = Vec::new();
        ui.push_id(("sankey-data",owner), |ui| {
            egui::CollapsingHeader::new(t("Weights")).default_open(sheet.sankey.is_some()).show(ui, |ui| {
                if sheet.sankey.is_none() {
                    if ui.button(t("Enable Sankey flows")).clicked() {
                        sheet.sankey = Some(Default::default());
                    }
                    ui.label(t("Connect topic names, notes and hyperlinks without changing mind-map parents."));
                    return;
                }
                let old_mode = sheet.sankey.as_ref().map(|s| s.mode).unwrap_or_default();
                let mut mode = old_mode;
                let mode_label = ui.label(t("Diagram mode"));
                egui::ComboBox::from_id_salt("flow-mode")
                    .selected_text(t(mode.label()))
                    .show_ui(ui, |ui| {
                        for item in SankeyMode::ALL {
                            ui.selectable_value(&mut mode, item, t(item.label()));
                        }
                    }).response.labelled_by(mode_label.id);
                if mode != old_mode {
                    // This is an explicit conversion action, never a reinterpretation.
                    if let Err(e) = sankey::convert_mode(&mut sheet, mode) {
                        error = Some(e);
                    }
                }
                ui.small(t("Mode changes reject lost source allocation or precision. Allocate unused source mass before converting."));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(t("Arrange flow cards")).clicked()
                        && let Err(e) = sankey::arrange(&mut sheet) {
                        error = Some(e);
                    }
                    if ui.button(t("Add flow node")).clicked() {
                        let at = primary.and_then(|id| original.topic(id)).and_then(|t| t.position)
                            .unwrap_or_default() + thinoda_core::Vec2::new(420.0,0.0);
                        match sheet.add_card(t("New flow node"),at,
                            thinoda_core::board::Card::new(thinoda_core::board::CardKind::Text,
                                thinoda_core::Vec2::new(155.0,100.0))) {
                            Ok(id) => {
                                focus_node = Some(id);
                                if let Some(s) = &mut sheet.sankey
                                    && s.mode == SankeyMode::Alluvial {
                                    let stage = primary.and_then(|id| s.stages.get(&id)).copied().unwrap_or(0);
                                    s.stages.insert(id,stage.saturating_add(1));
                                }
                                if let Some(s)=&mut sheet.sankey
                                    && s.mode==SankeyMode::Probability {s.sources.insert(id,0);}
                            }
                            Err(e) => error=Some(e),
                        }
                    }
                });
                if sheet.sankey.as_ref().is_some_and(|s| s.mode == SankeyMode::Alluvial) {
                    stage_overview(ui, &original, &mut sheet, &mut error, &mut focus_node);
                }
                let Some(settings) = &mut sheet.sankey else { return };
                ui.checkbox(&mut settings.color_scale, t("Weight color scale"));
                ui.small(t("The color scale overrides link colors. The legend appears on the canvas and in visual exports."));
                ui.horizontal(|ui| {
                    let label = ui.label(t("Width scale"));
                    if let Some(value)=number_edit(ui,ui.scope_id().with("width-scale"),label.id,f64::from(settings.pixels_per_unit),0.1..=10.0,1,&mut commits) {
                        settings.pixels_per_unit=value as f32;
                    }
                });
                if settings.color_scale {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(t("Overall mass"));
                        for value in sankey::WEIGHT_COLOR_SCALE_MARKS {
                            let c = sankey::weight_color(value);
                            let (rect,_) = ui.allocate_exact_size(egui::vec2(12.0,12.0),egui::Sense::hover());
                            ui.painter().rect_filled(rect,2.0,egui::Color32::from_rgb(c.r,c.g,c.b));
                            ui.label(format!("{value:.0}%"));
                        }
                    });
                    ui.small(t("Widths and numeric values carry the same information as color."));
                }
                let mode = settings.mode;
                if mode==SankeyMode::Alluvial {ui.small(t("Aggregate stage transitions do not preserve full cohort paths."));}
                if mode == SankeyMode::Probability {
                    let total = settings.sources.values().map(|v| sankey::percent(*v)).sum::<f64>();
                    ui.label(format!("{}: {total:.3}% · {}: {:.3}%",
                        t("Source mass"),t("Unallocated"),(100.0-total).max(0.0)));
                }
                if let Some(id) = primary {
                    let locked = node_locked(&original, id);
                    if mode == SankeyMode::Alluvial {
                        ui.add_enabled_ui(!locked, |ui| {
                            let mut assigned = settings.stages.contains_key(&id);
                            if ui.checkbox(&mut assigned,t("Assigned")).changed() {
                                if assigned { settings.stages.insert(id,0); }
                                else { settings.stages.remove(&id); }
                            }
                            let mut stage = settings.stages.get(&id).copied().unwrap_or(0);
                            ui.horizontal(|ui| {
                                let label = ui.label(t("Stage"));
                                if let Some(value)=number_edit(ui,ui.scope_id().with(("stage",id.0)),label.id,f64::from(stage),0.0..=1000.0,0,&mut commits) {
                                    stage=value as u32;
                                    settings.stages.insert(id,stage);
                                }
                            });
                        });
                    }
                    if mode == SankeyMode::Probability {
                        let has_parent = original.canvas.objects.iter().any(|o|
                            matches!(&o.kind, CanvasObjectKind::Connector(c)
                                if c.sankey.is_some() && c.to == Endpoint::Topic(id)));
                        ui.add_enabled_ui(!locked && !has_parent, |ui| {
                            let mut explicit = settings.sources.contains_key(&id);
                            if ui.checkbox(&mut explicit, t("Explicit source mass")).changed() {
                                if explicit { settings.sources.insert(id, 0); }
                                else { settings.sources.remove(&id); }
                            }
                            if let Some(value) = settings.sources.get_mut(&id) {
                                let percent = sankey::percent(*value);
                                ui.horizontal(|ui| {
                                    let label = ui.label(t("Source mass"));
                                    if let Some(percent)=number_edit(ui,ui.scope_id().with(("source",id.0)),label.id,percent,0.0..=100.0,3,&mut commits) {
                                        *value = (percent * 1000.0).round() as u32;
                                    }
                                });
                            }
                        });
                        ui.small(t("Parents show derived mass. Children edit conditional percentages. Sources are mutually exclusive."));
                        ui.small(t("New sources start at zero. Set Source mass before assigning conditional weights."));
                    }
                    let values = sankey::evaluate(&original).unwrap_or_default();
                    for parents in [true, false] {
                        ui.separator();
                        ui.strong(t(if parents { "Parents" } else { "Children" }));
                        let total = if parents { values.incoming.get(&id) } else { values.outgoing.get(&id) }
                            .copied().unwrap_or(0.0);
                        let total = if !parents && mode == SankeyMode::Probability {
                            original.canvas.objects.iter().filter_map(|o| match &o.kind {
                                CanvasObjectKind::Connector(c) if c.from == Endpoint::Topic(id) =>
                                    c.sankey.and_then(|f| f.weight).map(sankey::percent),
                                _ => None,
                            }).sum::<f64>()
                        } else { total };
                        ui.label(format!("{}: {total:.3}% · {}: {:.3}%",
                            t("Total"), t("Unallocated"), (100.0-total).max(0.0)));
                        for o in &mut sheet.canvas.objects {
                            let CanvasObjectKind::Connector(c) = &mut o.kind else { continue };
                            let Some(flow) = &mut c.sankey else { continue };
                            let other = match (c.from, c.to) {
                                (Endpoint::Topic(from), Endpoint::Topic(to)) if (parents && to == id) || (!parents && from == id) =>
                                    if parents { from } else { to },
                                _ => continue,
                            };
                            let name = node_label(&original,other);
                            let editable = !locked && !node_locked(&original, other)
                                && !original.canvas.is_locked(o.id);
                            ui.push_id(o.id.0, |ui| {
                                let weight_label = ui.label(format!("{name} — {}", t("Weight")));
                                ui.add_enabled_ui(editable, |ui| {
                                  if !(parents && mode == SankeyMode::Probability) {
                                    let mut assigned = flow.weight.is_some();
                                    if ui.checkbox(&mut assigned, t("Assigned")).changed() {
                                        flow.weight = assigned.then_some(0);
                                    }
                                    if let Some(value) = &mut flow.weight {
                                        let percent = sankey::percent(*value);
                                        if let Some(percent)=number_edit(ui,ui.scope_id().with(("weight",o.id.0,parents)),weight_label.id,percent,0.0..=100.0,3,&mut commits) {
                                            *value = (percent*1000.0).round() as u32;
                                        }
                                    }
                                  } else if flow.weight.is_none() {
                                      ui.label(t("Unassigned"));
                                  }
                                    let label = ui.label(t("Link label"));
                                    ui.text_edit_singleline(&mut c.label).labelled_by(label.id);
                                });
                                if mode == SankeyMode::Probability {
                                    ui.label(format!("{}: {:.3}%", t("Overall mass"), values.mass.get(&o.id).copied().unwrap_or(0.0)));
                                }
                                ui.horizontal(|ui| {
                                    if parents && mode==SankeyMode::Probability
                                        && ui.small_button(t("Edit upstream conditional")).clicked() {
                                        focus_node=Some(other);
                                    }
                                    if ui.small_button(t("Select link")).clicked() {
                                        focus_link = Some(o.id);
                                    }
                                    if ui.add_enabled(editable, egui::Button::new(t(if parents { "Remove parent link" } else { "Remove child link" })).small()).clicked() {
                                        removed.push(o.id);
                                    }
                                });
                                // Existing Style tab remains the full connector style editor.
                            });
                        }
                        let key = ui.scope_id().with(("add-flow", parents, id.0));
                        let mut other = ui.ctx().data_mut(|d| d.get_temp::<Option<TopicId>>(key).flatten());
                        let label = ui.label(t("Choose topic"));
                        egui::ComboBox::from_id_salt(key).selected_text(
                            other.map(|id|node_label(&original,id)).unwrap_or_else(||t("Choose topic").to_owned())
                        ).show_ui(ui, |ui| {
                            for &other_id in original.topics.keys() {
                                if !original.is_anchor(other_id) && (other_id != id || mode == SankeyMode::Circular) {
                                    ui.selectable_value(&mut other, Some(other_id), node_label(&original,other_id));
                                }
                            }
                        }).response.labelled_by(label.id);
                        ui.ctx().data_mut(|d| d.insert_temp(key, other));
                        let destination = if parents { Some(id) } else { other };
                        let replaces_source = mode == SankeyMode::Probability && destination.is_some_and(|id|
                            sheet.sankey.as_ref().is_some_and(|s| s.sources.contains_key(&id)));
                        let consent_key=key.with(("replace-source",destination));
                        let mut consent=ui.ctx().data_mut(|d| d.get_temp::<bool>(consent_key).unwrap_or(false));
                        if replaces_source {
                            ui.checkbox(&mut consent,t("Replace explicit source mass with this incoming link"));
                            ui.small(t("The new incoming flow becomes the only supply. Assign its weight after adding it."));
                            ui.ctx().data_mut(|d| d.insert_temp(consent_key,consent));
                        }
                        if ui.add_enabled(!locked && other.is_some() && (!replaces_source || consent), egui::Button::new(t(if parents { "Add parent" } else { "Add child" }))).clicked()
                            && let Some(other) = other
                        {
                            if node_locked(&original, other) {
                                error = Some(t("Locked").to_owned());
                            } else {
                                let (from,to) = if parents { (other,id) } else { (id,other) };
                                if replaces_source
                                    && let Some(s) = &mut sheet.sankey {
                                    s.sources.remove(&to);
                                }
                                if mode==SankeyMode::Probability
                                    && !sheet.canvas.objects.iter().any(|o|
                                        matches!(&o.kind,CanvasObjectKind::Connector(c) if c.sankey.is_some() && c.to==Endpoint::Topic(from)))
                                    && let Some(s)=&mut sheet.sankey {
                                    s.sources.entry(from).or_insert(0);
                                }
                                let mut c = CanvasConnector::new(Endpoint::Topic(from), Endpoint::Topic(to));
                                c.sankey = Some(Flow { weight: None });
                                c.arrow_end = false;
                                let mut object = CanvasObject::connector(c);
                                object.band = Band::Below;
                                if let Err(e) = sheet.add_canvas_object(object) {
                                    error = Some(e);
                                }
                                stage_new_alluvial_link(&mut sheet, &mut error);
                            }
                        }
                    }
                    let incoming = values.incoming.get(&id).copied().unwrap_or(0.0);
                    let outgoing = values.outgoing.get(&id).copied().unwrap_or(0.0);
                    if (incoming-outgoing).abs() > 0.000001 && mode != SankeyMode::Probability {
                        ui.label(format!("{}: {incoming:.3}% / {outgoing:.3}%", t("Imbalance (not normalized)")));
                    }
                } else if let Some(id) = selected
                    && let Some(object) = original.canvas.object(id)
                    && let CanvasObjectKind::Connector(c) = &object.kind
                    && let Ok((from,to)) = sankey::endpoints(c)
                {
                    ui.label(t("Select an endpoint topic to edit Parents and Children."));
                    ui.label(format!("{} / {}",node_label(&original,from),node_label(&original,to)));
                    let locked = node_locked(&original,from) || node_locked(&original,to) || original.canvas.is_locked(id);
                    let mut promoted=false;
                    if let Some(object) = sheet.canvas.object_mut(id)
                        && let CanvasObjectKind::Connector(c) = &mut object.kind {
                        ui.add_enabled_ui(!locked, |ui| {
                            let mut semantic=c.sankey.is_some();
                            if ui.checkbox(&mut semantic,t("Use as Sankey link")).changed() {
                                c.sankey=semantic.then_some(Flow { weight:None });
                                promoted=semantic;
                            }
                            if let Some(flow)=&mut c.sankey {
                                let mut assigned=flow.weight.is_some();
                                if ui.checkbox(&mut assigned,t("Assigned")).changed() {
                                    flow.weight=assigned.then_some(0);
                                }
                                if let Some(value)=&mut flow.weight {
                                    let label=ui.label(t("Weight"));
                                    let percent=sankey::percent(*value);
                                    if let Some(percent)=number_edit(ui,ui.scope_id().with(("selected-weight",id.0)),label.id,percent,0.0..=100.0,3,&mut commits) {
                                        *value=(percent*1000.0).round() as u32;
                                    }
                                }
                                if let Ok(values)=sankey::evaluate(&original) {
                                    ui.label(format!("{}: {:.3}%",t("Overall mass"),values.mass.get(&id).copied().unwrap_or(0.0)));
                                }
                            }
                        });
                    }
                    if promoted && sheet.sankey.as_ref().is_some_and(|s|s.mode==SankeyMode::Probability)
                        && !sheet.canvas.objects.iter().any(|o|
                            matches!(&o.kind,CanvasObjectKind::Connector(c) if c.sankey.is_some() && c.to==Endpoint::Topic(from)))
                        && let Some(s)=&mut sheet.sankey {
                        s.sources.entry(from).or_insert(0);
                    }
                    if promoted {
                        stage_new_alluvial_link(&mut sheet, &mut error);
                    }
                } else {
                    ui.label(t("Select a topic to edit its data."));
                }
            });
        });
        sheet.canvas.objects.retain(|o| !removed.contains(&o.id));
        if error.is_none() && sheet != original {
            let result = (|| {
                let doc = self.doc_mut().ok_or("No document")?;
                doc.commit_canvas_drafts()?;
                // A topic-title or connector-label draft can commit here.
                // Never replace that newly saved metadata with the stale
                // inspector snapshot; keep it and request a fresh flow edit.
                if doc.doc.sheet() != &original {
                    let current = doc.doc.sheet();
                    // Reapply this typed graph edit while retaining title/label
                    // drafts committed just above. Never replace their metadata.
                    let mut normalized = current.clone();
                    for (&id, topic) in &mut normalized.topics {
                        if let Some(old) = original.topic(id) {
                            if let Some(target) = sheet.topics.get_mut(&id)
                                && target.title == old.title
                            {
                                target.title = topic.title.clone();
                            }
                            topic.title = old.title.clone();
                        }
                    }
                    for object in &mut normalized.canvas.objects {
                        if let CanvasObjectKind::Connector(c) = &mut object.kind
                            && let Some(old) = original.canvas.object(object.id)
                            && let CanvasObjectKind::Connector(old) = &old.kind
                        {
                            if let Some(target) = sheet.canvas.object_mut(object.id)
                                && let CanvasObjectKind::Connector(target) = &mut target.kind
                                && target.label == old.label
                            {
                                target.label = c.label.clone();
                            }
                            c.label = old.label.clone();
                        }
                    }
                    if normalized != original {
                        return Err(t("The document changed. Try again.").to_owned());
                    }
                }
                doc.doc
                    .edit_sheet("Sankey weights", |target| {
                        *target = sheet;
                        Ok(())
                    })
                    .map_err(|e| e.to_string())
            })();
            if let Err(e) = result {
                error = Some(e);
            }
        }
        for key in commits {
            ui.ctx().data_mut(|d| {
                if let Some(mut draft) = d.get_temp::<NumberDraft>(key) {
                    if let Some(e) = &error {
                        draft.error = Some(e.clone());
                        d.insert_temp(key, draft);
                    } else {
                        d.remove::<NumberDraft>(key);
                    }
                }
            });
        }
        if error.is_none()
            && let Some(id) = focus_node
            && let Some(doc) = self.doc_mut()
        {
            doc.view.select_only(id);
        }
        if let Some(error) = error {
            self.toasts.error(error);
        }
        if let Some(id) = focus_link
            && let Some(doc) = self.doc_mut()
        {
            doc.view.select_board_object(id);
        }
    }
}
