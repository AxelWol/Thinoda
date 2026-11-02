//! A revision-bound, non-modal editor: navigating the map never retargets a draft.

use super::*;
use thinoda_core::{
    Sheet,
    fim::{
        FimCodeListReference, FimDocument, FimNodeDraft, FimNodeTarget, RecordMetadata, XmlField,
    },
};

#[cfg(test)]
#[path = "node_editor_tests.rs"]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Insertion {
    Child,
    Before,
    After,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    General,
    Constraints,
    Reuse,
    Associations,
}

pub(super) struct NodeEditor {
    owner: Owner,
    sheet_count: usize,
    peer_sheets: Vec<(usize, Sheet)>,
    source: FimDocument,
    original: Sheet,
    proposed_sheet: Option<Sheet>,
    proposal_label: Option<String>,
    target: FimNodeTarget,
    baseline_target: FimNodeTarget,
    draft: FimNodeDraft,
    baseline: FimNodeDraft,
    new_draft_cache: Option<FimNodeDraft>,
    typed: bool,
    page: Page,
    records: Vec<RecordMetadata>,
    choices: Vec<DefinitionChoice>,
    reuse_filter: String,
    initial_child_filter: String,
    rule_filter: String,
    shared_acknowledged: bool,
    advanced: Option<AdvancedEditor>,
    advanced_cache: Option<AdvancedEditor>,
    changes: Vec<FimChange>,
    error: Option<String>,
    close_requested: bool,
    delete_requested: bool,
    edit_after_create: bool,
    work: Option<NodeWork>,
}

impl NodeEditor {
    pub(super) fn show_structure(&mut self) {
        self.page = Page::Reuse;
    }
    fn typed_definition_changed(&self) -> bool {
        let mut definition = self.draft.clone();
        definition.cardinality = self.baseline.cardinality.clone();
        definition.usage_justification = self.baseline.usage_justification.clone();
        definition != self.baseline || self.independent()
    }
    fn definition_changed(&self) -> bool {
        self.typed_definition_changed() || !self.changes.is_empty()
    }

    fn reusing(&self) -> bool {
        matches!(self.target, FimNodeTarget::Insert { reuse: Some(_), .. })
    }
    fn changed(&self) -> bool {
        self.draft != self.baseline
            || self.target != self.baseline_target
            || !self.changes.is_empty()
            || self.unstaged_properties()
            || self.proposed_sheet.is_some()
    }
    fn unstaged_properties(&self) -> bool {
        self.advanced
            .as_ref()
            .or(self.advanced_cache.as_ref())
            .is_some_and(AdvancedEditor::unfinished)
    }
    fn record(&self) -> Option<RecordId> {
        match self.target {
            FimNodeTarget::Edit { record, .. } => Some(record),
            _ => None,
        }
    }
    fn independent(&self) -> bool {
        matches!(
            self.target,
            FimNodeTarget::Edit {
                independent: true,
                ..
            }
        )
    }
}

struct NodeWork {
    receiver: mpsc::Receiver<Result<(Sheet, ExchangeReport), String>>,
}

struct DefinitionChoice {
    index: usize,
    aliases: Vec<RecordId>,
}

fn definition_choices(source: &FimDocument, records: &[RecordMetadata]) -> Vec<DefinitionChoice> {
    let mut choices: Vec<DefinitionChoice> = Vec::new();
    let mut keys = std::collections::HashMap::new();
    for (index, record) in records.iter().enumerate() {
        let key = (
            std::mem::discriminant(&record.kind),
            record.identity.clone(),
            record.version.clone(),
            record.identity.is_none().then_some(record.record),
        );
        if let Some(&choice) = keys.get(&key) {
            let candidate: &mut DefinitionChoice = &mut choices[choice];
            if source.equivalent_definition(record.record, records[candidate.index].record) {
                candidate.aliases.push(record.record);
                continue;
            }
        }
        keys.entry(key).or_insert(choices.len());
        choices.push(DefinitionChoice {
            index,
            aliases: vec![record.record],
        });
    }
    choices
}

struct AdvancedEditor {
    fields: Vec<XmlField>,
    common: std::collections::BTreeMap<RecordId, String>,
    filter: String,
    page: usize,
    parent: RecordId,
    name: String,
    namespace: String,
    text: String,
    before: Option<RecordId>,
    attribute: String,
    fragment: String,
}

impl AdvancedEditor {
    fn unfinished(&self) -> bool {
        [&self.name, &self.text, &self.attribute, &self.fragment]
            .iter()
            .any(|value| !value.is_empty())
    }

    fn new(source: &FimDocument, record: RecordId) -> Self {
        let mut fields = source.node_fields(record);
        fields.extend(source.node_edge_fields(record));
        Self {
            common: common_fields(&fields, record),
            fields,
            filter: String::new(),
            page: 0,
            parent: record,
            name: String::new(),
            namespace: source.dialect().namespace().into(),
            text: String::new(),
            before: None,
            attribute: String::new(),
            fragment: String::new(),
        }
    }
}

impl ThinodaApp {
    fn fim_picker_matches(&self, doc: &crate::doc::OpenDoc) -> bool {
        self.image_request
            .as_ref()
            .is_some_and(|request| request.matches(doc))
            || self
                .attachment_request
                .as_ref()
                .is_some_and(|request| request.matches(doc))
    }

    pub(crate) fn accept_fim_canvas_preview(
        &mut self,
        ctx: &Context,
        preview: thinoda_core::Document,
    ) {
        self.accept_fim_preview(ctx, preview, false);
    }

    pub(crate) fn accept_fim_rendered_preview(
        &mut self,
        ctx: &Context,
        preview: thinoda_core::Document,
    ) {
        self.accept_fim_preview(ctx, preview, true);
    }

    fn accept_fim_preview(
        &mut self,
        ctx: &Context,
        preview: thinoda_core::Document,
        canvas_processed: bool,
    ) {
        let Some(doc) = self.doc() else { return };
        if preview.state_id() == doc.doc.state_id() {
            return;
        }
        let source_changed = doc
            .doc
            .sheet()
            .fim
            .as_ref()
            .zip(preview.sheet().fim.as_ref())
            .is_some_and(|(before, after)| !before.same_source_revision(after));
        if source_changed {
            if self.fim_node_has_draft() {
                self.toasts
                    .warning(t("Apply or cancel the open FIM node draft first."));
                return;
            }
            if let Err(error) = doc.ensure_no_pending_ink() {
                self.toasts.warning(error);
                return;
            }
            // A picker can complete after the review opens and edit this
            // same sheet, even though its on-screen controls are blocked.
            if self.fim_picker_matches(doc)
                || doc.view.editing_board_object().is_some()
                || doc.view.editing_topic().is_some()
                || doc.view.relationship_from.is_some()
                || doc.has_inspector_changes()
                || doc.fim_edit.is_some()
            {
                self.toasts.warning(t("Not possible here"));
                return;
            }
            // A hierarchy review blocks further map input. Never open it
            // while a board gesture still needs a release or a second
            // connector endpoint: doing so could discard that input.
            let pointer_in_flight = ctx
                .input(|input| input.pointer.any_down() || input.any_touches())
                || (!canvas_processed
                    && (ctx.dragged_id().is_some()
                        || ctx.drag_stopped_id().is_some()
                        || ctx.interaction_snapshot(|snapshot| {
                            snapshot.clicked == Some(thinoda_canvas::MapCanvas::id())
                        })));
            if pointer_in_flight || doc.view.board_tool == thinoda_canvas::BoardTool::Connector {
                self.toasts.warning(t("Not possible here"));
                return;
            }
            let record = doc.doc.sheet().fim.as_ref().and_then(|source| {
                source.record_for_topic(doc.view.primary().unwrap_or(doc.doc.sheet().root))
            });
            self.begin_fim_node(ctx, record, None);
            if let Some(editor) = self.fim.editor.as_mut() {
                editor.proposal_label = preview.undo_label().map(str::to_owned);
                editor.proposed_sheet = Some(preview.sheet().clone());
                editor.typed = false;
            }
        } else if let Some(doc) = self.doc_mut()
            && let Err(error) = doc.doc.edit_sheet(t("Edit"), |sheet| {
                *sheet = preview.sheet().clone();
                Ok(())
            })
        {
            self.toasts.error(error.to_string());
        }
    }
    pub(crate) fn set_fim_node_name(&mut self, text: &str) {
        if let Some(editor) = self.fim.editor.as_mut()
            && editor.typed
            && !editor.changed()
            && !editor.reusing()
            && editor.work.is_none()
        {
            editor.draft.name = text.to_owned();
        }
    }
    pub(crate) fn begin_fim_duplicate(&mut self, ctx: &Context) {
        if self.fim_node_has_draft() {
            self.toasts
                .warning(t("Apply or cancel the open FIM node draft first."));
            return;
        }
        let Some((record, draft)) = self.doc().and_then(|doc| {
            let source = doc.doc.sheet().fim.as_ref()?;
            let record = source.record_for_topic(doc.view.primary()?)?;
            Some((record, source.node_draft(record)))
        }) else {
            return;
        };
        let draft = match draft {
            Ok(draft) => draft,
            Err(error) => {
                self.toasts.error(error.to_string());
                return;
            }
        };
        self.begin_fim_node(ctx, None, Some(Insertion::After));
        if let Some(editor) = self.fim.editor.as_mut()
            && let FimNodeTarget::Insert { reuse, .. } = &mut editor.target
        {
            *reuse = Some(record);
            editor.new_draft_cache = Some(editor.draft.clone());
            editor.draft = draft;
        }
    }
    pub(crate) fn request_fim_node_delete(&mut self, ctx: &Context) {
        if self.fim_node_has_draft() {
            self.toasts
                .warning(t("Apply or cancel the open FIM node draft first."));
            return;
        }
        self.begin_fim_node(ctx, None, None);
        if let Some(editor) = self.fim.editor.as_mut() {
            editor.delete_requested = true;
        }
    }
    pub(crate) fn fim_node_has_draft(&self) -> bool {
        self.fim
            .editor
            .as_ref()
            .is_some_and(|editor| editor.changed() || editor.work.is_some())
    }

    pub(crate) fn fim_node_modal_open(&self) -> bool {
        self.fim
            .editor
            .as_ref()
            .is_some_and(|editor| editor.close_requested || editor.delete_requested)
    }

    pub(crate) fn fim_hierarchy_review_open(&self) -> bool {
        self.fim
            .editor
            .as_ref()
            .is_some_and(|editor| editor.proposed_sheet.is_some())
    }

    pub(crate) fn request_fim_draft_discard(&mut self) {
        if let Some(editor) = self.fim.editor.as_mut() {
            if editor.work.is_none() {
                editor.close_requested = true;
            } else {
                self.toasts.warning(t("Validating FIM changes…"));
            }
        }
    }

    pub(crate) fn begin_fim_node(
        &mut self,
        ctx: &Context,
        selected: Option<RecordId>,
        insertion: Option<Insertion>,
    ) {
        if self.fim_node_has_draft() {
            self.toasts
                .warning(t("Apply or cancel the open FIM node draft first."));
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc.flush_inspector_edit(ctx).and_then(|()| {
            doc.view
                .try_commit_edit(&mut doc.doc)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }) {
            self.toasts.error(error);
            return;
        }
        let Some(source) = doc.doc.sheet().fim.as_ref() else {
            return;
        };
        let selected = selected.or_else(|| {
            doc.view
                .primary()
                .and_then(|id| source.record_for_topic(id))
        });
        let Some(record) = selected else {
            self.toasts.warning(t("Select a FIM node first."));
            return;
        };
        let target = match insertion {
            None => FimNodeTarget::Edit {
                record,
                independent: false,
            },
            Some(Insertion::Child) => FimNodeTarget::Insert {
                parent: record,
                index: source.node_children(record).len(),
                initial_child: None,
                reuse: None,
            },
            Some(position) => {
                let Some(parent) = source.node_parent(record) else {
                    self.toasts
                        .warning(t("The root has no sibling insertion point."));
                    return;
                };
                let Some(index) = source
                    .node_children(parent)
                    .iter()
                    .position(|id| *id == record)
                else {
                    self.toasts.warning(t("Not possible here"));
                    return;
                };
                FimNodeTarget::Insert {
                    parent,
                    index: index + usize::from(position == Insertion::After),
                    initial_child: None,
                    reuse: None,
                }
            }
        };
        let (draft, typed) = match insertion {
            Some(_) => (
                new_draft(
                    source.dialect(),
                    if source
                        .record_metadata(record)
                        .is_some_and(|m| m.kind == FimKind::Field)
                    {
                        FimKind::Rule
                    } else {
                        FimKind::Field
                    },
                ),
                true,
            ),
            None => match source.node_draft(record) {
                Ok(draft) => (draft, true),
                Err(_) if record == source.root_record() => (
                    FimNodeDraft::new(source.dialect(), FimKind::Collection),
                    false,
                ),
                Err(error) => {
                    self.toasts.error(error.to_string());
                    return;
                }
            },
        };
        let records = source.records();
        let choices = definition_choices(source, &records);
        let editor = NodeEditor {
            owner: Owner {
                key: doc.key.clone(),
                sheet: doc.doc.active_index(),
                state: doc.doc.state_id(),
            },
            sheet_count: doc.doc.workbook().sheets.len(),
            peer_sheets: doc
                .doc
                .workbook()
                .sheets
                .iter()
                .enumerate()
                .filter(|(index, sheet)| {
                    *index != doc.doc.active_index()
                        && sheet.fim.as_ref().is_some_and(|other| {
                            source.same_source_revision(other) && source == other
                        })
                })
                .map(|(index, sheet)| (index, sheet.clone()))
                .collect(),
            source: source.clone(),
            original: doc.doc.sheet().clone(),
            proposed_sheet: None,
            proposal_label: None,
            baseline_target: target.clone(),
            target,
            baseline: draft.clone(),
            new_draft_cache: None,
            draft,
            typed,
            records,
            choices,
            reuse_filter: String::new(),
            initial_child_filter: String::new(),
            rule_filter: String::new(),
            page: Page::General,
            shared_acknowledged: false,
            advanced: None,
            advanced_cache: None,
            changes: Vec::new(),
            error: None,
            close_requested: false,
            delete_requested: false,
            edit_after_create: false,
            work: None,
        };
        self.fim.editor = Some(editor);
    }

    fn rebase_fim_node(&self, editor: &mut NodeEditor) {
        // A hierarchy proposal and an in-flight validation both contain their
        // own sheet snapshot; rebasing either could overwrite one of the edits.
        if editor.proposed_sheet.is_some() || editor.work.is_some() {
            return;
        }
        // fim_owner also excludes unresolved ink, whose samples must be
        // resolved before any FIM draft can be applied.
        let Some(owner) = self.fim_owner() else {
            return;
        };
        if owner.key != editor.owner.key
            || owner.sheet != editor.owner.sheet
            || owner.state == editor.owner.state
        {
            return;
        }
        let Some(doc) = self.doc() else {
            return;
        };
        let workbook = doc.doc.workbook();
        let sheet = doc.doc.sheet();
        if workbook.sheets.len() != editor.sheet_count || sheet.title != editor.original.title {
            return;
        }
        let Some(source) = sheet.fim.as_ref() else {
            return;
        };
        // In addition to the immutable source revision, require the same
        // topic bindings so the pinned node cannot be silently retargeted.
        if !editor.source.same_source_revision(source) || editor.source != *source {
            return;
        }
        // Sheet indices are not stable identities. A duplicate with the same
        // source and bindings could have replaced the owner after a reorder.
        // Only rebase if all such peer sheets still occupy their old slots.
        let peers: Vec<_> = workbook
            .sheets
            .iter()
            .enumerate()
            .filter(|(index, other)| {
                *index != owner.sheet
                    && other.fim.as_ref().is_some_and(|other_source| {
                        editor.source.same_source_revision(other_source)
                            && editor.source == *other_source
                    })
            })
            .collect();
        if peers.len() != editor.peer_sheets.len()
            || peers.iter().zip(&editor.peer_sheets).any(
                |((index, sheet), (old_index, old_sheet))| {
                    index != old_index || *sheet != old_sheet
                },
            )
        {
            return;
        }
        editor.original = sheet.clone();
        editor.owner = owner;
    }

    fn fim_node_interaction_pending(&self, ctx: &Context, editor: &NodeEditor) -> bool {
        self.doc().is_some_and(|doc| {
            doc.key == editor.owner.key
                && doc.doc.active_index() == editor.owner.sheet
                && (doc.has_unresolved_ink()
                    || doc.view.board_tool == thinoda_canvas::BoardTool::Connector
                    || doc.view.editing_board_object().is_some()
                    || doc.view.editing_topic().is_some()
                    || doc.view.relationship_from.is_some()
                    || doc.has_inspector_changes()
                    || doc.fim_edit.is_some()
                    || self.fim_picker_matches(doc)
                    || ctx.input(|input| input.pointer.any_down() || input.any_touches()))
        })
    }

    pub(super) fn fim_node_dialog(&mut self, ctx: &Context) {
        let Some(mut editor) = self.fim.editor.take() else {
            return;
        };
        self.rebase_fim_node(&mut editor);
        let current = self.fim_owner().as_ref() == Some(&editor.owner);
        // A successful worker replaces the whole sheet. Wait until pointer
        // gestures, label drafts and asynchronous pickers finish first, so
        // their pending input is not invalidated by that replacement.
        let finished = if self.fim_node_interaction_pending(ctx, &editor) {
            None
        } else {
            editor
                .work
                .as_ref()
                .and_then(|work| match work.receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err(t("FIM validation worker stopped").into()))
                    }
                    Err(mpsc::TryRecvError::Empty) => None,
                })
        };
        if let Some(result) = finished {
            editor.work.take().expect("completed node validation");
            match result {
                Ok((candidate, report)) if current => {
                    let inserted = matches!(editor.target, FimNodeTarget::Insert { .. })
                        .then(|| {
                            candidate
                                .fim
                                .as_ref()?
                                .records()
                                .into_iter()
                                .find(|record| {
                                    record.kind == editor.draft.kind
                                        && !editor
                                            .records
                                            .iter()
                                            .any(|old| old.record == record.record)
                                })
                                .map(|record| record.record)
                        })
                        .flatten();
                    let result = self
                        .doc_mut()
                        .ok_or_else(|| t("Not possible here").to_owned())
                        .and_then(|doc| {
                            doc.doc
                                .edit_sheet(t("Edit FIM node"), |sheet| {
                                    *sheet = candidate;
                                    Ok(())
                                })
                                .map_err(|error| error.to_string())
                        });
                    match result {
                        Ok(()) => {
                            self.fim.report = self.fim_owner().map(|owner| (owner, report));
                            self.fim.error = None;
                            if let Some(record) = inserted {
                                if let Some(doc) = self.doc_mut()
                                    && let Some(topic) = doc
                                        .doc
                                        .sheet()
                                        .fim
                                        .as_ref()
                                        .and_then(|source| source.topic_for_record(record))
                                {
                                    doc.view.select_only(topic);
                                }
                                if editor.edit_after_create {
                                    self.begin_fim_node(ctx, Some(record), None);
                                    if let Some(next) = self.fim.editor.as_mut() {
                                        next.advanced =
                                            Some(AdvancedEditor::new(&next.source, record));
                                    }
                                }
                            }
                            return;
                        }
                        Err(error) => editor.error = Some(error),
                    }
                }
                Ok(_) => editor.error = Some(t("Validation is stale").into()),
                Err(error) => editor.error = Some(error),
            }
        }
        // A stale worker result is never committed. Once it has finished,
        // preserve the draft on the latest compatible sheet so it can be
        // validated again instead of forcing the user to discard it.
        self.rebase_fim_node(&mut editor);
        let current = self.fim_owner().as_ref() == Some(&editor.owner);
        let busy = editor.work.is_some();
        let mut apply = false;
        let mut cancel = false;
        let mut show_target = false;
        let mut advanced = false;
        let mut delete = false;
        let mut create_more = false;
        let labels = crate::i18n::dialog_labels();
        let title = if editor.proposed_sheet.is_some() {
            "Review hierarchy changes"
        } else if matches!(editor.target, FimNodeTarget::Insert { .. }) {
            "Create FIM node"
        } else {
            "Edit FIM node"
        };
        let response = Dialog::new(egui::Id::unique("fim-node-editor"), t(title))
            .labels(&labels)
            // A staged hierarchy owns a complete sheet snapshot. Resolve it
            // before accepting board edits rather than overwriting either.
            .modal(editor.proposed_sheet.is_some())
            .default_width(690.0)
            .default_height(650.0)
            .min_size(egui::vec2(300.0, 280.0))
            .scroll(false)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!("{} · {}", kind_label(editor.draft.kind), editor.baseline.name));
                    show_target = ui.button(t("Show in map")).clicked();
                });
                widgets::caption(ui, t("This draft stays attached to this node while you navigate the map."));
                if !current {
                    ui.colored_label(ui.visuals().warn_fg_color, t("The document changed. The FIM draft was kept; cancel and reopen it."));
                }
                if editor.proposed_sheet.is_none() { ui.horizontal_wrapped(|ui| {
                    for (page, label) in [
                        (Page::General, "General"),
                        (Page::Constraints, "Field constraints"),
                        (Page::Reuse, "Reuse and structure"),
                        (Page::Associations, "Rules and code lists"),
                    ] {
                        ui.selectable_value(&mut editor.page, page, t(label));
                    }
                }); }
                ui.separator();
                let error_height = (ui.available_height() * 0.2).clamp(48.0, 100.0);
                let footer_height = if ui.available_width() < 500.0 { 155.0 } else { 120.0 }
                    + if editor.error.is_some() { error_height } else { 0.0 };
                let height = (ui.available_height() - footer_height).max(64.0);
                egui::ScrollArea::vertical().id_salt("fim-node-page").max_height(height).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.add_enabled_ui(!busy && editor.advanced.is_none(), |ui| if editor.proposed_sheet.is_some() {
                        if let Some(label) = &editor.proposal_label { ui.strong(label); }
                        widgets::caption(ui, t("Hierarchy changes are staged. Apply validates the whole document; Cancel keeps the original hierarchy."));
                        widgets::caption(ui, t("Apply or cancel the open FIM node draft first."));
                    } else if !editor.typed {
                        widgets::caption(ui, t("Use More schema properties to edit message metadata without expanding the node tree."));
                    } else { match editor.page {
                        Page::General => { ui.add_enabled_ui(!editor.reusing(), |ui| general_page(ui, &mut editor)); },
                        Page::Constraints => { ui.add_enabled_ui(!editor.reusing(), |ui| constraints_page(ui, &mut editor)); },
                        Page::Reuse => reuse_page(ui, &mut editor),
                        Page::Associations => { ui.add_enabled_ui(!editor.reusing(), |ui| associations_page(ui, &mut editor)); },
                    } });
                });
                ui.separator();
                if let Some(error) = &editor.error {
                    egui::ScrollArea::vertical().id_salt("fim-node-error").max_height(error_height).show(ui, |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(error).color(ui.visuals().error_fg_color)).wrap());
                    });
                }
                if editor.proposed_sheet.is_none() { ui.horizontal_wrapped(|ui| {
                    let pages = [Page::General, Page::Constraints, Page::Reuse, Page::Associations];
                    let index = pages.iter().position(|page| *page == editor.page).expect("wizard page");
                    if ui.add_enabled(index > 0, egui::Button::new(t("Previous"))).clicked() { editor.page = pages[index - 1]; }
                    if ui.add_enabled(index + 1 < pages.len(), egui::Button::new(t("Next"))).clicked() { editor.page = pages[index + 1]; }
                }); }
                if busy {
                    ui.horizontal(|ui| { ui.spinner(); ui.label(t("Validating FIM changes…")); });
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                ui.horizontal_wrapped(|ui| {
                    let enabled = current && !busy && editor.advanced.is_none();
                    apply = ui.add_enabled(enabled, egui::Button::new(t("Apply"))).clicked();
                    cancel = ui.add_enabled(!busy, egui::Button::new(t("Cancel"))).clicked();
                    if editor.record().is_some() && editor.proposed_sheet.is_none() {
                        advanced = ui.add_enabled(!busy, egui::Button::new(t("More schema properties…"))).clicked();
                    } else if editor.record().is_none() && !editor.reusing() {
                        create_more = ui.add_enabled(!busy && current, egui::Button::new(t("Create and edit more properties…"))).clicked();
                        apply |= create_more;
                    }
                    delete = ui.add_enabled(enabled && editor.typed && editor.record().is_some() && editor.proposed_sheet.is_none(), egui::Button::new(t("Delete FIM node…"))).clicked();
                });
            });
        let touch = widgets::touch_activation_allowed(ctx);
        if !touch {
            apply = false;
            cancel = false;
            advanced = false;
            delete = false;
            show_target = false;
            create_more = false;
        }
        if show_target
            && current
            && let Some(record) = editor.record()
            && let Some(topic) = editor.source.topic_for_record(record)
            && let Some(doc) = self.doc_mut()
        {
            doc.view.select_only(topic);
        }
        if advanced && let Some(record) = editor.record() {
            editor.advanced = editor
                .advanced_cache
                .take()
                .or_else(|| Some(AdvancedEditor::new(&editor.source, record)));
        }
        advanced_dialog(ctx, &mut editor);
        if (cancel || response.close_requested) && !busy {
            if editor.changed() {
                editor.close_requested = true;
            } else {
                return;
            }
        }
        editor.delete_requested |= delete;
        if editor.close_requested {
            let mut discard = false;
            let mut keep = false;
            let result = Dialog::new(
                egui::Id::unique("fim-node-discard"),
                t("Discard FIM draft?"),
            )
            .labels(&labels)
            .modal(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(t(
                    "Only the unapplied node draft is discarded. The document is unchanged.",
                ));
                ui.horizontal_wrapped(|ui| {
                    keep = ui.button(t("Keep editing")).clicked();
                    discard = ui.button(t("Discard draft")).clicked();
                });
            });
            if touch && discard {
                return;
            }
            if touch && (keep || result.close_requested) {
                editor.close_requested = false;
            }
        }
        if editor.delete_requested {
            let mut confirm = false;
            let mut keep = false;
            let result = Dialog::new(egui::Id::unique("fim-node-delete"), t("Delete FIM node?"))
                .labels(&labels).modal(true).default_width(440.0).show(ctx, |ui| {
                    ui.label(t("Delete this occurrence and its contained nodes? Shared parent usages are affected. Dangling references and invalid schemas block deletion."));
                    ui.horizontal_wrapped(|ui| {
                        keep = ui.button(t("Cancel")).clicked();
                        confirm = ui.add_enabled(current && !busy, egui::Button::new(t("Delete FIM node"))).clicked();
                    });
                });
            if touch && (keep || result.close_requested) {
                editor.delete_requested = false;
            }
            if touch && confirm {
                editor.delete_requested = false;
                if let Some(record) = editor.record() {
                    let change = editor
                        .source
                        .topic_for_record(record)
                        .map_or(FimChange::RemoveElement { record }, |topic| {
                            FimChange::Remove { topic }
                        });
                    self.start_fim_node_apply(ctx, &mut editor, Some(change));
                }
            }
        } else if apply {
            editor.edit_after_create = create_more;
            self.start_fim_node_apply(ctx, &mut editor, None);
        }
        self.fim.editor = Some(editor);
    }

    fn start_fim_node_apply(
        &mut self,
        ctx: &Context,
        editor: &mut NodeEditor,
        delete: Option<FimChange>,
    ) {
        if delete.is_none() && editor.unstaged_properties() {
            editor.error =
                Some(t("Stage or clear unfinished schema properties before applying.").into());
            return;
        }
        if self.fim_owner().as_ref() != Some(&editor.owner) {
            editor.error = Some(
                t("The document changed. The FIM draft was kept; cancel and reopen it.").into(),
            );
            return;
        }
        let usages = editor
            .record()
            .and_then(|record| editor.records.iter().find(|r| r.record == record))
            .map_or(1, |r| r.usages);
        if delete.is_none() && usages > 1 && !editor.independent() && !editor.shared_acknowledged {
            editor.page = Page::Reuse;
            editor.error = Some(
                t("Confirm the shared-definition impact or make this occurrence independent.")
                    .into(),
            );
            return;
        }
        let dialect = io_dialect(editor.source.dialect());
        let Some(validator) = self.fim.schemas.validator_handle(dialect) else {
            editor.error =
                Some(t("Install the official schemas before applying FIM changes.").into());
            self.fim.consent = Some(dialect);
            return;
        };
        let mut candidate = editor
            .proposed_sheet
            .as_ref()
            .unwrap_or(&editor.original)
            .clone();
        let deleting = delete.is_some();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let xdf3 = editor.source.dialect() == thinoda_core::fim::Dialect::Xdf3;
        let mut draft = editor.draft.clone();
        if !xdf3
            && draft.kind == FimKind::CodeList
            && let Some(reference) = &mut draft.code_list
        {
            reference.identity = draft.identity.clone();
            reference.version = draft.version.clone();
        }
        if xdf3
            && !deleting
            && editor.typed
            && editor.typed_definition_changed()
            && editor.draft.last_changed == editor.baseline.last_changed
            && matches!(
                editor.draft.kind,
                FimKind::Field | FimKind::Group | FimKind::Choice | FimKind::Rule | FimKind::Schema
            )
        {
            draft.last_changed = Some(timestamp.clone());
        }
        let parent = match editor.target {
            FimNodeTarget::Insert { parent, .. } => Some(parent),
            FimNodeTarget::Edit { record, .. }
                if deleting
                    || editor.independent()
                    || editor.draft.identity != editor.baseline.identity
                    || editor.draft.version != editor.baseline.version
                    || editor.draft.cardinality != editor.baseline.cardinality
                    || editor.draft.usage_justification != editor.baseline.usage_justification
                    || editor.draft.kind == FimKind::CodeList && editor.definition_changed() =>
            {
                editor.source.node_parent(record)
            }
            _ => None,
        };
        let changed_reference = (!deleting
            && !editor.independent()
            && (editor.draft.identity != editor.baseline.identity
                || editor.draft.version != editor.baseline.version
                || editor.draft.kind == FimKind::CodeList && editor.definition_changed()))
        .then(|| editor.record())
        .flatten();
        let change = delete.or_else(|| {
            editor.typed.then(|| FimChange::AuthorNode {
                target: editor.target.clone(),
                draft: Box::new(draft),
                timestamp: timestamp.clone(),
            })
        });
        let mut changes: Vec<_> = change.into_iter().collect();
        if !deleting {
            changes.extend(editor.changes.clone());
        }
        let original = editor.source.clone();
        let raw_date_owners: Vec<_> = editor
            .changes
            .iter()
            .filter_map(|change| {
                let record = match change {
                    FimChange::SetText { record, .. }
                    | FimChange::SetAttribute { record, .. }
                    | FimChange::RemoveElement { record } => *record,
                    FimChange::InsertElement { parent, .. }
                    | FimChange::InsertFragment { parent, .. } => *parent,
                    _ => return None,
                };
                if let Some(field) = original
                    .node_fields(record)
                    .into_iter()
                    .find(|field| field.record == record)
                {
                    let unchanged = match change {
                        FimChange::SetText { value, .. } => {
                            !field.has_elements && field.value == *value
                        }
                        FimChange::SetAttribute { name, value, .. } => {
                            field
                                .attributes
                                .iter()
                                .find(|attribute| attribute.name == *name)
                                .map(|attribute| &attribute.value)
                                == value.as_ref()
                        }
                        _ => false,
                    };
                    if unchanged {
                        return None;
                    }
                }
                let owner = if original.record_metadata(record).is_some_and(|metadata| {
                    matches!(
                        metadata.kind,
                        FimKind::Schema
                            | FimKind::Group
                            | FimKind::Choice
                            | FimKind::Field
                            | FimKind::Rule
                    )
                }) {
                    record
                } else {
                    original.node_parent(record)?
                };
                if Some(owner) == editor.record()
                    && editor.draft.last_changed != editor.baseline.last_changed
                {
                    None
                } else {
                    Some(owner)
                }
            })
            .collect();
        let (sender, receiver) = mpsc::channel();
        let context = ctx.clone();
        let task = move || {
            let result = validator
                .lock()
                .map_err(|e| e.to_string())
                .and_then(|validator| {
                    if xdf3 {
                        let mut parents: Vec<_> = parent.into_iter().collect();
                        for owner in raw_date_owners {
                            if !parents
                                .iter()
                                .any(|known| original.equivalent_definition(*known, owner))
                            {
                                parents.push(owner);
                            }
                        }
                        if let Some(after) = candidate.fim.as_ref() {
                            for record in original.records().into_iter().filter(|record| {
                                matches!(
                                    record.kind,
                                    FimKind::Schema | FimKind::Group | FimKind::Choice
                                )
                            }) {
                                if after.record_metadata(record.record).is_some()
                                    && original.node_children(record.record)
                                        != after.node_children(record.record)
                                    && !parents.iter().any(|known| {
                                        after.equivalent_definition(*known, record.record)
                                    })
                                {
                                    parents.push(record.record);
                                }
                            }
                        }
                        if let Some(record) = changed_reference {
                            for alias in original.records().into_iter().filter(|alias| {
                                original.equivalent_definition(alias.record, record)
                            }) {
                                if let Some(parent) = original.node_parent(alias.record)
                                    && !parents
                                        .iter()
                                        .any(|known| original.equivalent_definition(*known, parent))
                                {
                                    parents.push(parent);
                                }
                            }
                        }
                        for record in parents {
                            let mut draft = candidate
                                .fim
                                .as_ref()
                                .ok_or("Missing FIM source")?
                                .node_draft(record)
                                .map_err(|error| error.to_string())?;
                            draft.last_changed = Some(timestamp.clone());
                            changes.insert(
                                0,
                                FimChange::AuthorNode {
                                    target: FimNodeTarget::Edit {
                                        record,
                                        independent: false,
                                    },
                                    draft: Box::new(draft),
                                    timestamp: timestamp.clone(),
                                },
                            );
                        }
                    }
                    candidate
                        .apply_fim_change(FimChange::Batch(changes))
                        .map_err(|e| e.to_string())?;
                    let bytes =
                        thinoda_io::fim::export_document(&candidate).map_err(|e| e.to_string())?;
                    let baseline = validator
                        .validate(&original.serialize())
                        .map_err(|e| e.to_string())?;
                    let report = validator.validate(&bytes).map_err(|e| e.to_string())?;
                    if !report.is_exchange_conformant() {
                        // Repair work cannot introduce a new XSD error, and never approves XDF export.
                        let new_errors = new_error_messages(
                            baseline
                                .diagnostics
                                .iter()
                                .map(|d| (d.message.as_str(), d.path.as_deref())),
                            report
                                .diagnostics
                                .iter()
                                .map(|d| (d.message.as_str(), d.path.as_deref())),
                        );
                        if baseline.is_exchange_conformant()
                            || !new_errors.is_empty()
                            || report.omitted_diagnostics > 0
                        {
                            return Err(format!(
                                "{}\n{}",
                                t("Validation failed"),
                                new_errors.join("\n")
                            ));
                        }
                    }
                    Ok((candidate, report))
                });
            if sender.send(result).is_ok() {
                context.request_repaint();
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(error) = std::thread::Builder::new()
            .name("fim-node-validation".into())
            .spawn(task)
        {
            editor.error = Some(error.to_string());
            return;
        }
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            task();
        });
        editor.work = Some(NodeWork { receiver });
        editor.error = None;
    }
}

fn io_dialect(dialect: thinoda_core::fim::Dialect) -> Dialect {
    match dialect {
        thinoda_core::fim::Dialect::Xdf2 => Dialect::Xdf2,
        thinoda_core::fim::Dialect::Xdf3 => Dialect::Xdf3,
    }
}

fn new_draft(dialect: thinoda_core::fim::Dialect, kind: FimKind) -> FimNodeDraft {
    let mut draft = FimNodeDraft::new(dialect, kind);
    draft.name = kind_name(kind).into();
    if draft.input_label.is_some() {
        draft.input_label = Some(draft.name.clone());
    }
    if draft.output_label.is_some() {
        draft.output_label = Some(draft.name.clone());
    }
    draft
}

fn text(ui: &mut Ui, label: &str, value: &mut String, multiline: bool) {
    ui.label(label);
    let edit = if multiline {
        egui::TextEdit::multiline(value).desired_rows(3)
    } else {
        egui::TextEdit::singleline(value)
    };
    ui.add(edit.desired_width(ui.available_width()))
        .accessible_name(label);
}

fn optional_text(ui: &mut Ui, label: &str, value: &mut Option<String>, multiline: bool) {
    let mut content = value.clone().unwrap_or_default();
    ui.label(label);
    let edit = if multiline {
        egui::TextEdit::multiline(&mut content).desired_rows(3)
    } else {
        egui::TextEdit::singleline(&mut content)
    };
    if ui
        .add(edit.desired_width(ui.available_width()))
        .accessible_name(label)
        .changed()
    {
        *value = (!content.is_empty()).then_some(content);
    }
}

fn code(ui: &mut Ui, label: &str, value: &mut Option<String>, choices: &[(&str, &'static str)]) {
    egui::ComboBox::from_label(label)
        .selected_text(
            choices
                .iter()
                .find(|(id, _)| value.as_deref() == Some(*id))
                .map_or_else(|| value.as_deref().unwrap_or("—"), |(_, name)| t(name)),
        )
        .show_ui(ui, |ui| {
            for (id, name) in choices {
                ui.selectable_value(value, Some((*id).into()), t(name));
            }
        });
}

fn general_page(ui: &mut Ui, editor: &mut NodeEditor) {
    if matches!(editor.target, FimNodeTarget::Insert { reuse: None, .. }) {
        let mut kind = editor.draft.kind;
        let parent_kind = if let FimNodeTarget::Insert { parent, .. } = editor.target {
            editor
                .records
                .iter()
                .find(|r| r.record == parent)
                .map(|r| r.kind)
        } else {
            None
        };
        let kinds = if parent_kind == Some(FimKind::Field) {
            &[FimKind::Rule, FimKind::CodeList][..]
        } else {
            &[FimKind::Field, FimKind::Group, FimKind::Rule][..]
        };
        egui::ComboBox::from_label(t("Node type"))
            .selected_text(kind_name(kind))
            .show_ui(ui, |ui| {
                for &next in kinds {
                    ui.selectable_value(
                        &mut kind,
                        next,
                        format!("{} · {}", kind_label(next), kind_name(next)),
                    );
                }
            });
        if kind != editor.draft.kind {
            let defaults = new_draft(editor.source.dialect(), kind);
            let d = &mut editor.draft;
            if d.name == kind_name(d.kind) {
                for label in [&mut d.input_label, &mut d.output_label] {
                    if label.as_deref() == Some(&d.name) {
                        *label = Some(defaults.name.clone());
                    }
                }
                d.name = defaults.name;
            }
            d.kind = kind;
            d.schema_element_type = d
                .schema_element_type
                .take()
                .or(defaults.schema_element_type);
            d.field_type = d.field_type.take().or(defaults.field_type);
            d.datatype = d.datatype.take().or(defaults.datatype);
            d.rule_type = d.rule_type.take().or(defaults.rule_type);
            d.rule_script = d.rule_script.take().or(defaults.rule_script);
            d.input_label = d.input_label.take().or(defaults.input_label);
            d.input_help = d.input_help.take().or(defaults.input_help);
            d.output_help = d.output_help.take().or(defaults.output_help);
        }
    } else {
        ui.label(format!(
            "{}: {} · {}",
            t("Node type"),
            kind_label(editor.draft.kind),
            kind_name(editor.draft.kind)
        ));
    }
    let d = &mut editor.draft;
    if d.kind == FimKind::CodeList && editor.source.dialect() == thinoda_core::fim::Dialect::Xdf3 {
        widgets::caption(
            ui,
            t("XDF3 code lists use canonical references, not FIM registry IDs."),
        );
    } else {
        optional_text(ui, t("Assigned FIM ID"), &mut d.identity, false);
        optional_text(ui, t("Version"), &mut d.version, false);
    }
    if d.kind == FimKind::CodeList {
        if editor.source.dialect() == thinoda_core::fim::Dialect::Xdf2 {
            let reference = d.code_list.get_or_insert_with(Default::default);
            reference.identity = d.identity.clone();
            reference.version = d.version.clone();
        }
        widgets::caption(
            ui,
            t("Edit the canonical reference on the Rules and code lists page."),
        );
        return;
    }
    let previous_name = d.name.clone();
    text(ui, t("Name"), &mut d.name, false);
    if matches!(editor.target, FimNodeTarget::Insert { reuse: None, .. }) && d.name != previous_name
    {
        if d.input_label.as_deref() == Some(previous_name.as_str())
            || previous_name.is_empty() && d.input_label.is_none()
        {
            d.input_label = Some(d.name.clone());
        }
        if d.output_label.as_deref() == Some(previous_name.as_str()) {
            d.output_label = Some(d.name.clone());
        }
    }
    optional_text(ui, t("Description"), &mut d.description, true);
    let rule3 =
        d.kind == FimKind::Rule && editor.source.dialect() == thinoda_core::fim::Dialect::Xdf3;
    if !rule3 {
        optional_text(ui, t("Definition"), &mut d.definition, true);
        optional_text(ui, t("Input label"), &mut d.input_label, false);
        optional_text(ui, t("Output label"), &mut d.output_label, false);
        optional_text(ui, t("Input help text"), &mut d.input_help, true);
        optional_text(ui, t("Output help text"), &mut d.output_help, true);
    }
}

fn constraints_page(ui: &mut Ui, editor: &mut NodeEditor) {
    let d = &mut editor.draft;
    if matches!(d.kind, FimKind::Field | FimKind::Group | FimKind::Choice) {
        code(
            ui,
            t("Structure element type"),
            &mut d.schema_element_type,
            &[
                ("RNG", "Legal basis"),
                ("HAR", "Harmonized"),
                ("ABS", "Abstract"),
            ],
        );
    }
    if matches!(d.kind, FimKind::Group | FimKind::Choice)
        && editor.source.dialect() == thinoda_core::fim::Dialect::Xdf3
    {
        ui.checkbox(&mut d.group_choice, t("Choice group"));
    }
    if d.kind == FimKind::Field {
        let xdf3 = editor.source.dialect() == thinoda_core::fim::Dialect::Xdf3;
        let field_types = if xdf3 {
            &[
                ("input", "Input"),
                ("label", "Static"),
                ("select", "Selection"),
                ("hidden", "Hidden"),
                ("locked", "Locked"),
            ][..]
        } else {
            &[
                ("input", "Input"),
                ("label", "Static"),
                ("select", "Selection"),
            ][..]
        };
        code(ui, t("Field type"), &mut d.field_type, field_types);
        let mut datatypes = vec![
            ("text", "Text"),
            ("bool", "Boolean"),
            ("date", "Date"),
            ("num", "Decimal number"),
            ("num_int", "Integer"),
            ("num_currency", "Currency"),
            ("file", "File"),
            ("obj", "Object"),
        ];
        if xdf3 {
            datatypes.extend([
                ("datetime", "Date and time"),
                ("time", "Time"),
                ("text_latin", "String.Latin+"),
            ]);
        }
        code(ui, t("Data type"), &mut d.datatype, &datatypes);
        optional_text(ui, t("Default value"), &mut d.content, true);
        if xdf3 {
            code(
                ui,
                t("Prefill"),
                &mut d.prefill,
                &[
                    ("keine", "None"),
                    ("optional", "Optional"),
                    ("verpflichtend", "Required"),
                ],
            );
            if d.datatype.as_deref() == Some("file")
                || d.max_size.is_some()
                || !d.media_types.is_empty()
            {
                optional_text(ui, t("Maximum file size (bytes)"), &mut d.max_size, false);
                let mut media = d.media_types.join("\n");
                let before = media.clone();
                text(ui, t("Allowed media types"), &mut media, true);
                if media != before {
                    d.media_types = media
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
            }
        }
        ui.separator();
        ui.strong(t("Precision"));
        if xdf3 {
            optional_text(ui, t("Minimum length"), &mut d.precision.min_length, false);
            optional_text(ui, t("Maximum length"), &mut d.precision.max_length, false);
            optional_text(ui, t("Minimum value"), &mut d.precision.min_value, false);
            optional_text(ui, t("Maximum value"), &mut d.precision.max_value, false);
            optional_text(ui, t("Pattern"), &mut d.precision.pattern, false);
        } else {
            optional_text(ui, t("Precision"), &mut d.precision.text, true);
        }
    } else if d.kind == FimKind::Rule {
        if editor.source.dialect() == thinoda_core::fim::Dialect::Xdf3 {
            code(
                ui,
                t("Rule type"),
                &mut d.rule_type,
                &[
                    ("V", "Validation"),
                    ("M", "Multiplicity"),
                    ("B", "Calculation"),
                    ("K", "Complex rule"),
                ],
            );
        }
        optional_text(ui, t("Rule text"), &mut d.rule_text, true);
        optional_text(
            ui,
            t("Rule script (not executed)"),
            &mut d.rule_script,
            true,
        );
    } else {
        widgets::caption(
            ui,
            t("Only properties defined for this node type are changed."),
        );
    }
}

fn record_picker(
    ui: &mut Ui,
    label: &str,
    selected: &mut Option<RecordId>,
    records: &[RecordMetadata],
    choices: &[DefinitionChoice],
    kinds: &[FimKind],
    filter: &mut String,
) -> bool {
    let before = *selected;
    egui::ComboBox::from_label(label)
        .selected_text(
            selected
                .and_then(|id| choices.iter().find(|choice| choice.aliases.contains(&id)))
                .map(|choice| &records[choice.index])
                .map_or(t("None"), |r| r.name.as_str()),
        )
        .show_ui(ui, |ui| {
            ui.add(egui::TextEdit::singleline(filter).hint_text(t("Search")))
                .accessible_name(label);
            ui.selectable_value(selected, None, t("None"));
            let query = filter.trim().to_lowercase();
            let visible: Vec<_> = choices
                .iter()
                .filter(|choice| {
                    kinds.contains(&records[choice.index].kind)
                        && record_matches(&records[choice.index], &query)
                })
                .collect();
            ui.scope(|ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                egui::ScrollArea::vertical().max_height(280.0).show_rows(
                    ui,
                    ui.spacing().interact_size.y,
                    visible.len(),
                    |ui, rows| {
                        for index in rows {
                            let choice = visible[index];
                            let record = &records[choice.index];
                            ui.selectable_value(
                                selected,
                                Some(record.record),
                                format!(
                                    "{} · {} · {}",
                                    kind_label(record.kind),
                                    record.name,
                                    record.identity.as_deref().unwrap_or("")
                                ),
                            )
                            .on_hover_text(format!(
                                "{}: {} · #{}",
                                t("Shared definition"),
                                choice.aliases.len(),
                                record.record.0
                            ));
                        }
                    },
                );
            });
        });
    before != *selected
}

fn record_matches(record: &RecordMetadata, query: &str) -> bool {
    query.is_empty()
        || [
            &record.name,
            record.identity.as_deref().unwrap_or(""),
            record.version.as_deref().unwrap_or(""),
        ]
        .iter()
        .any(|value| value.to_lowercase().contains(query))
}

fn reuse_page(ui: &mut Ui, editor: &mut NodeEditor) {
    if editor.reusing() {
        ui.label(t(
            "Reusing a definition preserves its metadata. Edit it after insertion.",
        ));
    }
    if let Some(record) = editor.record() {
        if let Some(metadata) = editor.records.iter().find(|r| r.record == record) {
            ui.label(format!("{}: {}", t("Shared definition"), metadata.usages));
            let bob = metadata
                .identity
                .as_deref()
                .is_some_and(|id| id.get(1..3) == Some("60"));
            if bob {
                ui.strong(t("BOB number range (60)"));
            }
            widgets::caption(
                ui,
                t(
                    "Reuse evidence comes from IDs and preserved relations; it does not prove registry provenance.",
                ),
            );
            if metadata.usages > 1 {
                ui.checkbox(
                    &mut editor.shared_acknowledged,
                    t("Update all shared usages"),
                );
            }
        }
        if let FimNodeTarget::Edit { independent, .. } = &mut editor.target {
            if ui
                .checkbox(independent, t("Make this occurrence independent"))
                .changed()
            {
                editor.shared_acknowledged = false;
                let canonical = editor.draft.kind == FimKind::CodeList
                    && editor.source.dialect() == thinoda_core::fim::Dialect::Xdf3;
                if canonical {
                    if *independent && editor.draft.code_list == editor.baseline.code_list {
                        if let Some(reference) = &mut editor.draft.code_list {
                            reference.canonical_identification = None;
                            reference.canonical_version_uri = None;
                            reference.genericode_version = None;
                        }
                    } else if !*independent {
                        editor.draft.code_list = editor.baseline.code_list.clone();
                    }
                } else if *independent {
                    if editor.draft.identity == editor.baseline.identity {
                        editor.draft.identity = None;
                    }
                    if editor.draft.version == editor.baseline.version {
                        editor.draft.version = None;
                    }
                } else {
                    editor.draft.identity = editor.baseline.identity.clone();
                    editor.draft.version = editor.baseline.version.clone();
                }
            }
            if *independent
                && editor.draft.kind == FimKind::CodeList
                && editor.source.dialect() == thinoda_core::fim::Dialect::Xdf3
            {
                widgets::caption(
                    ui,
                    t("Edit the canonical reference on the Rules and code lists page."),
                );
            } else if *independent {
                optional_text(ui, t("Assigned FIM ID"), &mut editor.draft.identity, false);
                optional_text(ui, t("Version"), &mut editor.draft.version, false);
                widgets::caption(
                    ui,
                    t("A new assigned ID is required. Nested reuse and provenance are preserved."),
                );
            }
        }
    }
    if let FimNodeTarget::Insert {
        parent,
        index,
        initial_child,
        reuse,
    } = &mut editor.target
    {
        ui.label(format!(
            "{}: {}",
            t("Parent"),
            editor
                .records
                .iter()
                .find(|r| r.record == *parent)
                .map_or("—", |r| &r.name)
        ));
        let count = editor.source.node_children(*parent).len();
        ui.horizontal(|ui| {
            ui.label(t("Insertion position"));
            ui.add(egui::DragValue::new(index).range(0..=count));
        });
        let previous_reuse = *reuse;
        let edge = (
            editor.draft.cardinality.clone(),
            editor.draft.usage_justification.clone(),
        );
        let kinds = if editor
            .records
            .iter()
            .any(|record| record.record == *parent && record.kind == FimKind::Field)
        {
            &[FimKind::Rule, FimKind::CodeList][..]
        } else {
            &[
                FimKind::Field,
                FimKind::Group,
                FimKind::Choice,
                FimKind::Rule,
            ][..]
        };
        if record_picker(
            ui,
            t("Reuse existing definition"),
            reuse,
            &editor.records,
            &editor.choices,
            kinds,
            &mut editor.reuse_filter,
        ) {
            if let Some(record) = *reuse {
                match editor.source.node_draft(record) {
                    Ok(draft) => {
                        if previous_reuse.is_none() {
                            editor.new_draft_cache = Some(editor.draft.clone());
                        }
                        editor.draft = draft;
                    }
                    Err(error) => {
                        *reuse = previous_reuse;
                        editor.error = Some(error.to_string());
                    }
                }
            } else {
                editor.draft = editor
                    .new_draft_cache
                    .take()
                    .unwrap_or_else(|| new_draft(editor.source.dialect(), editor.draft.kind));
            }
            editor.draft.cardinality = edge.0;
            editor.draft.usage_justification = edge.1;
        }
        if editor.draft.kind == FimKind::Group
            && editor.source.dialect() == thinoda_core::fim::Dialect::Xdf3
            && reuse.is_none()
        {
            widgets::caption(
                ui,
                t("XDF3 groups require an initial child. Choose an existing field or group."),
            );
            record_picker(
                ui,
                t("Initial child"),
                initial_child,
                &editor.records,
                &editor.choices,
                &[FimKind::Field, FimKind::Group, FimKind::Choice],
                &mut editor.initial_child_filter,
            );
        }
    }
    if editor.draft.cardinality.is_some()
        || matches!(editor.target, FimNodeTarget::Insert { .. })
            && matches!(
                editor.draft.kind,
                FimKind::Field | FimKind::Group | FimKind::Choice
            )
    {
        optional_text(
            ui,
            t("Multiplicity (Min:Max)"),
            &mut editor.draft.cardinality,
            false,
        );
        optional_text(
            ui,
            t("Usage justification"),
            &mut editor.draft.usage_justification,
            true,
        );
        widgets::caption(
            ui,
            t(
                "Multiplicity and usage justification belong to the incoming structure, not the definition.",
            ),
        );
    }
}

fn associations_page(ui: &mut Ui, editor: &mut NodeEditor) {
    let d = &mut editor.draft;
    if matches!(
        d.kind,
        FimKind::Field | FimKind::Group | FimKind::Choice | FimKind::Schema
    ) {
        ui.strong(t("Associated rules"));
        ui.add(egui::TextEdit::singleline(&mut editor.rule_filter).hint_text(t("Search")))
            .accessible_name(t("Associated rules"));
        let query = editor.rule_filter.trim().to_lowercase();
        let used_rules: std::collections::HashSet<_> = d.rules.iter().copied().collect();
        let visible: Vec<_> = editor
            .choices
            .iter()
            .filter(|choice| {
                editor.records[choice.index].kind == FimKind::Rule
                    && record_matches(&editor.records[choice.index], &query)
            })
            .collect();
        ui.scope(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            egui::ScrollArea::vertical().max_height(280.0).show_rows(
                ui,
                ui.spacing().interact_size.y,
                visible.len(),
                |ui, rows| {
                    for index in rows {
                        let choice = visible[index];
                        let rule = &editor.records[choice.index];
                        let mut used = choice
                            .aliases
                            .iter()
                            .any(|record| used_rules.contains(record));
                        if ui
                            .checkbox(
                                &mut used,
                                format!(
                                    "{} · {}",
                                    rule.name,
                                    rule.identity.as_deref().unwrap_or("")
                                ),
                            )
                            .changed()
                        {
                            if used {
                                d.rules.push(rule.record);
                            } else {
                                d.rules.retain(|id| !choice.aliases.contains(id));
                            }
                        }
                    }
                },
            );
        });
        if editor.records.iter().all(|r| r.kind != FimKind::Rule) {
            widgets::caption(ui, t("No rule definitions in this document."));
        }
    }
    if matches!(d.kind, FimKind::Field | FimKind::CodeList) {
        ui.separator();
        let mut bound = d.code_list.is_some();
        if d.kind == FimKind::CodeList {
            ui.strong(t("Code list reference"));
            d.code_list.get_or_insert_with(Default::default);
        } else if ui.checkbox(&mut bound, t("Code list reference")).changed() {
            d.code_list = bound.then(FimCodeListReference::default);
        }
        if let Some(reference) = &mut d.code_list {
            if editor.source.dialect() == thinoda_core::fim::Dialect::Xdf2 {
                optional_text(ui, t("Assigned FIM ID"), &mut reference.identity, false);
                optional_text(ui, t("Version"), &mut reference.version, false);
            }
            optional_text(
                ui,
                t("Canonical identification"),
                &mut reference.canonical_identification,
                false,
            );
            optional_text(
                ui,
                t("Code list version"),
                &mut reference.genericode_version,
                false,
            );
            optional_text(
                ui,
                t("Canonical version URI"),
                &mut reference.canonical_version_uri,
                false,
            );
            optional_text(ui, t("Code column"), &mut reference.code_key, false);
            optional_text(ui, t("Name column"), &mut reference.name_key, false);
            optional_text(ui, t("Help column"), &mut reference.help_key, false);
            widgets::caption(
                ui,
                t("External code lists are referenced, never downloaded automatically."),
            );
            if d.kind == FimKind::CodeList
                && editor.source.dialect() == thinoda_core::fim::Dialect::Xdf2
            {
                d.identity = reference.identity.clone();
                d.version = reference.version.clone();
            }
        }
    }
}

fn stage(changes: &mut Vec<FimChange>, change: FimChange) {
    let same_property = |old: &FimChange| match (old, &change) {
        (FimChange::SetText { record: a, .. }, FimChange::SetText { record: b, .. }) => a == b,
        (
            FimChange::SetAttribute {
                record: a, name: x, ..
            },
            FimChange::SetAttribute {
                record: b, name: y, ..
            },
        ) => a == b && x == y,
        (FimChange::RemoveElement { record: a }, FimChange::RemoveElement { record: b }) => a == b,
        _ => false,
    };
    changes.retain(|old| !same_property(old));
    changes.push(change);
}

fn common_fields(
    fields: &[XmlField],
    root: RecordId,
) -> std::collections::BTreeMap<RecordId, String> {
    let local = |name: &str| name.rsplit(':').next().unwrap_or(name).to_owned();
    let mut common = std::collections::BTreeMap::new();
    for field in fields.iter().filter(|field| !field.has_elements) {
        let name = local(&field.name);
        let parent = fields
            .iter()
            .find(|parent| Some(parent.record) == field.parent);
        let property = if field.parent == Some(root) {
            Some(name)
        } else if let Some(parent) = parent.filter(|parent| parent.parent == Some(root)) {
            match (local(&parent.name).as_str(), name.as_str()) {
                ("identifikation", "id") => Some("identity".into()),
                ("identifikation", "version") => Some("version".into()),
                (kind @ ("feldart" | "datentyp" | "schemaelementart" | "typ"), "code") => {
                    Some(kind.into())
                }
                _ => None,
            }
        } else if name == "anzahl" {
            Some("anzahl".into())
        } else {
            None
        };
        if let Some(property) = property {
            common.insert(field.record, property);
        }
    }
    common
}

fn common_value<'a>(draft: &'a mut FimNodeDraft, property: &str) -> Option<&'a mut Option<String>> {
    match property {
        "identity" => Some(&mut draft.identity),
        "version" => Some(&mut draft.version),
        "beschreibung" => Some(&mut draft.description),
        "definition" => Some(&mut draft.definition),
        "bezeichnungEingabe" => Some(&mut draft.input_label),
        "bezeichnungAusgabe" => Some(&mut draft.output_label),
        "hilfetextEingabe" => Some(&mut draft.input_help),
        "hilfetextAusgabe" => Some(&mut draft.output_help),
        "feldart" => Some(&mut draft.field_type),
        "datentyp" => Some(&mut draft.datatype),
        "schemaelementart" => Some(&mut draft.schema_element_type),
        "praezisierung" => Some(&mut draft.precision.text),
        "anzahl" => Some(&mut draft.cardinality),
        "freitextRegel" => Some(&mut draft.rule_text),
        "script" | "skript" => Some(&mut draft.rule_script),
        "typ" => Some(&mut draft.rule_type),
        "inhalt" => Some(&mut draft.content),
        "vorbefuellung" => Some(&mut draft.prefill),
        "maxSize" => Some(&mut draft.max_size),
        "letzteAenderung" => Some(&mut draft.last_changed),
        _ => None,
    }
}

fn new_error_messages<'a>(
    baseline: impl Iterator<Item = (&'a str, Option<&'a str>)>,
    candidate: impl Iterator<Item = (&'a str, Option<&'a str>)>,
) -> Vec<String> {
    let mut counts = std::collections::HashMap::new();
    for signature in baseline {
        *counts.entry(signature).or_insert(0usize) += 1;
    }
    candidate
        .filter_map(|signature| {
            if let Some(count) = counts.get_mut(&signature)
                && *count > 0
            {
                *count -= 1;
                None
            } else {
                Some(signature.1.map_or_else(
                    || signature.0.to_owned(),
                    |path| format!("{path}: {}", signature.0),
                ))
            }
        })
        .collect()
}

fn precision_attribute<'a>(
    draft: &'a mut FimNodeDraft,
    name: &str,
) -> Option<&'a mut Option<String>> {
    match name {
        "minLength" => Some(&mut draft.precision.min_length),
        "maxLength" => Some(&mut draft.precision.max_length),
        "minValue" => Some(&mut draft.precision.min_value),
        "maxValue" => Some(&mut draft.precision.max_value),
        "pattern" => Some(&mut draft.precision.pattern),
        _ => None,
    }
}

fn advanced_dialog(ctx: &Context, editor: &mut NodeEditor) {
    let Some(mut advanced) = editor.advanced.take() else {
        return;
    };
    let labels = crate::i18n::dialog_labels();
    let mut close = false;
    let response = Dialog::new(egui::Id::unique("fim-node-advanced"), t("More schema properties"))
        .labels(&labels).modal(false).default_width(620.0).default_height(590.0)
        .min_size(egui::vec2(300.0, 250.0)).show(ctx, |ui| {
            widgets::caption(ui, t("Advanced source editing. Changes are staged with the node and validated before Apply."));
            if let Some(last) = editor.changes.last() {
                let property = match last {
                    FimChange::SetText { record, .. } | FimChange::SetAttribute { record, .. }
                    | FimChange::RemoveElement { record } => advanced.fields.iter().find(|field| field.record == *record).map_or_else(|| format!("#{}", record.0), |field| field.path.clone()),
                    FimChange::InsertElement { name, .. } => name.clone(),
                    FimChange::InsertFragment { .. } => t("XML fragment").into(),
                    _ => t("More schema properties").into(),
                };
                if ui.button(format!("{}: {property}", t("Undo"))).clicked() {
                    editor.changes.pop();
                    advanced.fields = editor.source.node_fields(editor.record().expect("advanced editor target"));
                    for change in &editor.changes {
                        match change {
                            FimChange::SetText { record, value } => {
                                if let Some(field) = advanced.fields.iter_mut().find(|field| field.record == *record) { field.value = value.clone(); }
                            }
                            FimChange::SetAttribute { record, name, value } => {
                                if let Some(field) = advanced.fields.iter_mut().find(|field| field.record == *record) {
                                    if let Some(value) = value {
                                        if let Some(attribute) = field.attributes.iter_mut().find(|attribute| attribute.name == *name) { attribute.value = value.clone(); }
                                    } else { field.attributes.retain(|attribute| attribute.name != *name); }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            if ui.add(egui::TextEdit::singleline(&mut advanced.filter).hint_text(t("Search"))).changed() { advanced.page = 0; }
            let filter = advanced.filter.to_lowercase();
            let indices: Vec<_> = advanced.fields.iter().enumerate().filter(|(_, f)| filter.is_empty() || f.path.to_lowercase().contains(&filter)).map(|(i, _)| i).collect();
            const PAGE_SIZE: usize = 24;
            advanced.page = advanced.page.min(indices.len().saturating_sub(1) / PAGE_SIZE);
            ui.horizontal(|ui| {
                if ui.add_enabled(advanced.page > 0, egui::Button::new(t("Previous"))).clicked() { advanced.page -= 1; }
                ui.label(format!("{} / {}", advanced.page + 1, indices.len().div_ceil(PAGE_SIZE).max(1)));
                if ui.add_enabled((advanced.page + 1) * PAGE_SIZE < indices.len(), egui::Button::new(t("Next"))).clicked() { advanced.page += 1; }
            });
            for index in indices.into_iter().skip(advanced.page * PAGE_SIZE).take(PAGE_SIZE) {
                let field = &mut advanced.fields[index];
                ui.push_id(field.record.0, |ui| {
                    egui::CollapsingHeader::new(&field.name).show(ui, |ui| {
                        ui.small(&field.path);
                        if !field.has_elements {
                            let property = advanced.common.get(&field.record);
                            let common = if editor.typed { property.and_then(|property| {
                                if property == "name" {
                                    field.value = editor.draft.name.clone();
                                    Some(property.as_str())
                                } else if let Some(value) = common_value(&mut editor.draft, property) {
                                    field.value = value.clone().unwrap_or_default();
                                    Some(property.as_str())
                                } else { None }
                            }) } else { None };
                            if ui.add(egui::TextEdit::multiline(&mut field.value).desired_rows(2).desired_width(ui.available_width())).accessible_name(&field.path).changed() {
                                if common == Some("name") { editor.draft.name = field.value.clone(); }
                                else if let Some(value) = common.and_then(|property| common_value(&mut editor.draft, property)) {
                                    *value = Some(field.value.clone());
                                } else {
                                    stage(&mut editor.changes, FimChange::SetText { record: field.record, value: field.value.clone() });
                                }
                            }
                        }
                        for attribute in &mut field.attributes {
                            ui.label(format!("@{}", attribute.name));
                            if attribute.name == "xmlns" || attribute.name.starts_with("xmlns:") {
                                ui.small(&attribute.value);
                                continue;
                            }
                            let common = editor.typed && editor.source.dialect() == thinoda_core::fim::Dialect::Xdf3
                                && field.name.rsplit(':').next() == Some("praezisierung");
                            if common && let Some(value) = precision_attribute(&mut editor.draft, &attribute.name) {
                                attribute.value = value.clone().unwrap_or_default();
                            }
                            if ui.add(egui::TextEdit::singleline(&mut attribute.value).desired_width(ui.available_width())).accessible_name(format!("{} @{}", field.path, attribute.name)).changed() {
                                if let Some(value) = common.then(|| precision_attribute(&mut editor.draft, &attribute.name)).flatten() {
                                    *value = (!attribute.value.is_empty()).then(|| attribute.value.clone());
                                } else {
                                    stage(&mut editor.changes, FimChange::SetAttribute { record: field.record, name: attribute.name.clone(), value: Some(attribute.value.clone()) });
                                }
                            }
                            if ui.small_button(t("Remove attribute")).clicked() {
                                if let Some(value) = common.then(|| precision_attribute(&mut editor.draft, &attribute.name)).flatten() { *value = None; }
                                else { stage(&mut editor.changes, FimChange::SetAttribute { record: field.record, name: attribute.name.clone(), value: None }); }
                            }
                        }
                        if ui.button(t("Use as insertion parent")).clicked() { advanced.parent = field.record; advanced.before = None; }
                        if Some(field.record) != editor.record() && ui.button(t("Remove schema property")).clicked() {
                            if let Some(value) = advanced.common.get(&field.record).filter(|_| editor.typed)
                                .and_then(|property| common_value(&mut editor.draft, property)) { *value = None; }
                            else { stage(&mut editor.changes, FimChange::RemoveElement { record: field.record }); }
                        }
                    });
                });
            }
            ui.separator();
            ui.strong(t("Add schema property"));
            let children: Vec<_> = advanced.fields.iter().filter(|f| f.parent == Some(advanced.parent)).collect();
            egui::ComboBox::from_label(t("Insert before")).selected_text(advanced.before.and_then(|id| children.iter().find(|f| f.record == id)).map_or(t("Append"), |f| &f.name)).show_ui(ui, |ui| {
                ui.selectable_value(&mut advanced.before, None, t("Append"));
                for field in &children { ui.selectable_value(&mut advanced.before, Some(field.record), &field.name); }
            });
            text(ui, t("XML element name"), &mut advanced.name, false);
            text(ui, t("Namespace"), &mut advanced.namespace, false);
            text(ui, t("Value"), &mut advanced.text, true);
            if ui.add_enabled(!advanced.name.trim().is_empty(), egui::Button::new(t("Stage property"))).clicked() {
                editor.changes.push(FimChange::InsertElement { parent: advanced.parent, before: advanced.before, name: advanced.name.clone(), namespace: advanced.namespace.clone(), text: Some(advanced.text.clone()) });
                advanced.name.clear(); advanced.text.clear();
            }
            text(ui, t("Attribute"), &mut advanced.attribute, false);
            if ui.add_enabled(!advanced.attribute.trim().is_empty(), egui::Button::new(t("Stage attribute"))).clicked() {
                stage(&mut editor.changes, FimChange::SetAttribute { record: advanced.parent, name: advanced.attribute.clone(), value: Some(advanced.text.clone()) });
                advanced.attribute.clear(); advanced.text.clear();
            }
            egui::CollapsingHeader::new(t("XML fragment")).show(ui, |ui| {
                widgets::caption(ui, t("Insert one complete schema element with explicit namespaces. FIM definitions must use the node wizard."));
                text(ui, t("XML fragment"), &mut advanced.fragment, true);
                if ui.add_enabled(!advanced.fragment.trim().is_empty(), egui::Button::new(t("Stage XML fragment"))).clicked() {
                    editor.changes.push(FimChange::InsertFragment { parent: advanced.parent, before: advanced.before, xml: advanced.fragment.clone() });
                    advanced.fragment.clear();
                }
            });
            close = ui.button(t("Done")).clicked();
        });
    if !(widgets::touch_activation_allowed(ctx) && (close || response.close_requested)) {
        editor.advanced = Some(advanced);
    } else {
        editor.advanced_cache = Some(advanced);
    }
}
