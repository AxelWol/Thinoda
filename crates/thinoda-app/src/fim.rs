//! FIM authoring controls and exact-output validation lifecycle.

use std::{path::PathBuf, sync::mpsc};

use egui::{Context, Ui};
use thinoda_core::fim::{DependencyResolution, DiagnosticSeverity, FimChange, FimKind, RecordId};
use thinoda_io::{
    Format,
    fim::{Dialect, ExchangeReport},
};
use thinoda_ui::{
    Dialog,
    tokens::{Palette, Tokens},
    widgets,
};

use crate::{
    app::ThinodaApp,
    files,
    fim_schemas::{FimSchemas, SchemaStatus},
    i18n::t,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Owner {
    key: String,
    sheet: usize,
    state: u64,
}

struct ValidationWork {
    owner: Owner,
    receiver: mpsc::Receiver<Result<ExchangeReport, String>>,
    export: Option<(PathBuf, Vec<u8>)>,
}

#[derive(Default)]
pub(crate) struct FimState {
    pub(crate) schemas: FimSchemas,
    pub(crate) consent: Option<Dialect>,
    manual: Option<Dialect>,
    work: Option<ValidationWork>,
    report: Option<(Owner, ExchangeReport)>,
    error: Option<String>,
    selected_record: Option<(String, usize, RecordId)>,
    filter: String,
    page: usize,
    new_name: String,
    new_element: String,
    new_namespace: String,
    new_attribute: String,
    reuse: Option<RecordId>,
    context: Option<Context>,
    selection: Option<(String, usize, Option<thinoda_core::TopicId>)>,
    insert_before: Option<RecordId>,
    occurrence_only: bool,
    independent_identity: String,
    independent_version: String,
}

impl FimState {
    pub(crate) fn new(paths: Option<&thinoda_platform::Paths>) -> Self {
        Self {
            schemas: FimSchemas::new(paths),
            ..Default::default()
        }
    }
}

impl ThinodaApp {
    pub(crate) fn select_fim_record(&mut self, ctx: &Context, record: RecordId) {
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc.flush_inspector_edit(ctx).and_then(|()| {
            doc.view
                .try_commit_edit(&mut doc.doc)
                .map(|_| ())
                .map_err(|error| error.to_string())
        }) {
            self.toasts.error(error);
            return;
        }
        let owner = (doc.key.clone(), doc.doc.active_index(), doc.view.primary());
        self.fim.selected_record = Some((owner.0.clone(), owner.1, record));
        self.fim.selection = Some(owner);
        self.fim.page = 0;
        self.format_tab = crate::app::FormatTab::Fim;
        self.sidebar_family = crate::app::SidebarFamily::Format;
        self.settings.show_format_panel = true;
    }

    fn fim_owner(&self) -> Option<Owner> {
        self.doc().map(|doc| Owner {
            key: doc.key.clone(),
            sheet: doc.doc.active_index(),
            state: doc.doc.state_id(),
        })
    }

    pub(crate) fn poll_fim(&mut self, ctx: &Context) {
        self.fim.context = Some(ctx.clone());
        self.fim.schemas.poll(ctx);
        let result = self
            .fim
            .work
            .as_ref()
            .and_then(|work| match work.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("FIM validation worker stopped".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = result {
            let work = self.fim.work.take().expect("completed validation work");
            if self.fim_owner().as_ref() != Some(&work.owner) {
                self.fim.error = Some(t("Validation is stale").into());
                return;
            }
            match result {
                Ok(report) => {
                    if let Some((path, bytes)) = work.export {
                        if report.is_exchange_conformant() {
                            let written = self
                                .doc()
                                .ok_or_else(|| t("Not possible here").to_owned())
                                .and_then(|doc| doc.ensure_export_destination(&path))
                                .and_then(|()| {
                                    thinoda_platform::fs::atomic_write(&path, &bytes, false)
                                        .map_err(|error| error.to_string())
                                });
                            match written {
                                Ok(()) => {
                                    self.deliver(&path);
                                    self.toasts.success(format!(
                                        "{} {}",
                                        t("Exported to"),
                                        path.display()
                                    ));
                                }
                                Err(error) => self.fim.error = Some(error),
                            }
                        } else {
                            self.fim.error = Some(t("Validation failed").into());
                        }
                    }
                    self.fim.report = Some((work.owner, report));
                }
                Err(error) => self.fim.error = Some(error),
            }
        }
    }

    pub(crate) fn install_fim_schema_files(&mut self, paths: &[PathBuf]) {
        let Some(dialect) = self.fim.manual.take() else {
            return;
        };
        let expected = thinoda_io::fim::official_downloads(dialect);
        if paths.len() != expected.len() {
            self.fim.error = Some(format!(
                "Expected {} official ZIP archive(s)",
                expected.len()
            ));
            return;
        }
        let exchange = paths.iter().find(|path| {
            path.file_name()
                .is_some_and(|name| name == expected[0].artifact)
        });
        let (exchange, supplement) = if let Some(exchange) = exchange {
            (
                exchange.clone(),
                paths.iter().find(|path| *path != exchange).cloned(),
            )
        } else {
            (paths[0].clone(), paths.get(1).cloned())
        };
        if !self
            .fim
            .schemas
            .install_paths(dialect, exchange, supplement)
        {
            self.fim.error = Some(t("Not possible here").into());
        }
    }

    pub(crate) fn fim_dialogs(&mut self, ctx: &Context) -> bool {
        let Some(dialect) = self.fim.consent else {
            return false;
        };
        let mut download = false;
        let mut manual = false;
        let mut cancel = false;
        let labels = crate::i18n::dialog_labels();
        let response = Dialog::new(egui::Id::unique("fim_schema_consent"), t("Download official schemas?"))
            .labels(&labels)
            .default_width(510.0)
            .show(ctx, |ui| {
                ui.label(t("Only official schema archives are downloaded from XRepository. Your document stays on this device. Validation then works offline."));
                for artifact in thinoda_io::fim::official_downloads(dialect) {
                    ui.label(artifact.artifact);
                    ui.small(artifact.url);
                    ui.monospace(artifact.sha256);
                }
                ui.label(match dialect { Dialect::Xdf2 => "~20 KB", Dialect::Xdf3 => "~49 KB" });
                ui.horizontal_wrapped(|ui| {
                    cancel = ui.button(t("Cancel")).clicked();
                    manual = ui.button(t("Install from ZIP files")).clicked();
                    download = ui.button(t("Download and install")).clicked();
                });
            });
        if !widgets::touch_activation_allowed(ctx) {
            return true;
        }
        if response.close_requested || cancel {
            self.fim.consent = None;
        } else if download {
            self.fim.consent = None;
            if !self.fim.schemas.request_download(dialect) {
                self.fim.error = Some(t("Not possible here").into());
            }
        } else if manual {
            self.fim.consent = None;
            self.fim.manual = Some(dialect);
            if let Some(paths) = files::pick_files(
                files::Purpose::FimSchemas,
                Some(t("Install from ZIP files")),
                &[("ZIP", &["zip"])],
                None,
                true,
            ) {
                self.install_fim_schema_files(&paths);
            }
        }
        true
    }

    fn schema_controls(&mut self, ui: &mut Ui, dialect: Dialect) {
        self.fim.schemas.ensure_cached(dialect);
        let busy = self.fim.schemas.busy();
        match self.fim.schemas.status(dialect).clone() {
            SchemaStatus::Ready => {}
            SchemaStatus::Unavailable(error) => {
                ui.colored_label(ui.visuals().error_fg_color, t("Validation unavailable"));
                ui.small(error);
            }
            _ if busy => {
                ui.spinner();
            }
            _ => {
                ui.label(t("Not checked"));
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new(t("Install official schemas")))
                .clicked()
            {
                self.fim.consent = Some(dialect);
            }
            if ui
                .add_enabled(!busy, egui::Button::new(t("Retry")))
                .clicked()
            {
                self.fim.schemas.check_cached(dialect);
            }
            if ui
                .add_enabled(!busy, egui::Button::new(t("Remove")))
                .clicked()
            {
                self.fim.schemas.remove_cached(dialect);
                self.fim.report = None;
            }
        });
    }

    fn start_fim_validation(&mut self, ctx: &Context, export: Option<(PathBuf, Vec<u8>)>) {
        if self.fim.work.is_some() {
            self.fim.error = Some(t("Not possible here").into());
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc
            .view
            .try_commit_edit(&mut doc.doc)
            .map_err(|error| error.to_string())
            .and_then(|_| doc.flush_inspector_edit(ctx))
        {
            self.fim.error = Some(error);
            return;
        }
        let bytes = match thinoda_io::fim::export_document(doc.doc.sheet()) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.fim.error = Some(error.to_string());
                return;
            }
        };
        if let Some((_, requested)) = &export
            && *requested != bytes
        {
            self.fim.error = Some(t("Validation is stale").into());
            return;
        }
        if export.is_some()
            && doc.doc.sheet().fim.as_ref().is_some_and(|fim| {
                fim.diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
            })
        {
            self.fim.error = Some(t("Validation failed").into());
            return;
        }
        let profile = match thinoda_io::fim::inspect(&bytes) {
            Ok(profile) => profile,
            Err(error) => {
                self.fim.error = Some(error.to_string());
                return;
            }
        };
        let Some(validator) = self.fim.schemas.validator_handle(profile.dialect) else {
            self.fim.error = Some(t("Validation unavailable").into());
            self.fim.consent = Some(profile.dialect);
            return;
        };
        let Some(owner) = self.fim_owner() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let context = ctx.clone();
        let task = move || {
            let result = validator
                .lock()
                .map_err(|error| error.to_string())
                .and_then(|validator| {
                    validator
                        .validate(&bytes)
                        .map_err(|error| error.to_string())
                });
            let _ = sender.send(result);
            context.request_repaint();
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(error) = std::thread::Builder::new()
            .name("fim-validation".into())
            .spawn(task)
        {
            self.fim.error = Some(error.to_string());
            return;
        }
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            task();
        });
        self.fim.work = Some(ValidationWork {
            owner,
            receiver,
            export,
        });
        self.fim.error = None;
    }

    pub(crate) fn export_fim(&mut self, format: Format) {
        let ctx = self.fim_context();
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc
            .view
            .try_commit_edit(&mut doc.doc)
            .map_err(|error| error.to_string())
            .and_then(|_| doc.commit_inspector_edit())
        {
            self.fim.error = Some(error);
            return;
        }
        let target = match format {
            Format::FimXdf2 => Dialect::Xdf2,
            Format::FimXdf3 => Dialect::Xdf3,
            _ => return,
        };
        let assessment = match thinoda_io::fim::assess_conversion(doc.doc.sheet(), target) {
            Ok(assessment) => assessment,
            Err(error) => {
                self.fim.error = Some(error.to_string());
                return;
            }
        };
        if !assessment.supported {
            self.fim.error = Some(assessment.reasons.join("\n"));
            self.format_tab = crate::app::FormatTab::Fim;
            self.settings.show_format_panel = true;
            return;
        }
        let bytes = match thinoda_io::fim::export_document(doc.doc.sheet()) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.fim.error = Some(error.to_string());
                return;
            }
        };
        if self.fim.schemas.validator(target).is_none() {
            self.fim.consent = Some(target);
            return;
        }
        let Some(doc) = self.doc() else { return };
        let extension = match target {
            Dialect::Xdf2 => "xdf2.xml",
            Dialect::Xdf3 => "xdf3.xml",
        };
        let Some(path) = files::save_target(
            Some(t("Export validated XDF")),
            &[(format.label(), &["xml"])],
            &format!("{}.{}", crate::app::sanitize(&doc.title()), extension),
            doc.path.as_ref().and_then(|path| path.parent()),
            files::Target::Download,
        ) else {
            return;
        };
        self.start_fim_validation(&ctx, Some((path, bytes)));
    }

    fn fim_context(&self) -> Context {
        self.fim.context.clone().unwrap_or_default()
    }

    fn apply_fim(&mut self, ctx: &Context, change: FimChange) {
        let independent = matches!(&change, FimChange::MakeIndependent { .. });
        let Some(doc) = self.doc_mut() else { return };
        let result = doc.flush_inspector_edit(ctx).and_then(|()| {
            doc.doc
                .edit_sheet("FIM data", |sheet| {
                    sheet
                        .apply_fim_change(change)
                        .map_err(|error| thinoda_core::EditError::Invalid(error.to_string()))
                })
                .map_err(|error| error.to_string())
        });
        if let Err(error) = result {
            self.fim.error = Some(error);
        } else {
            self.fim.error = None;
            if independent {
                self.fim.independent_identity.clear();
                self.fim.independent_version.clear();
            }
        }
    }

    pub(crate) fn fim_tab(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.fim.context = Some(ctx.clone());
        let Some(doc) = self.doc() else { return };
        let Some(source) = doc.doc.sheet().fim.as_ref() else {
            widgets::caption(ui, t("Open an XDF2 or XDF3 document to edit FIM data."));
            return;
        };
        let key = doc.key.clone();
        let index = doc.doc.active_index();
        let primary = doc.view.primary();
        let source = source.clone();
        let selection = (key.clone(), index, primary);
        let selection_changed = self.fim.selection.as_ref() != Some(&selection);
        if selection_changed {
            self.fim.selected_record = None;
            self.fim.page = 0;
            self.fim.selection = Some(selection);
            self.fim.occurrence_only = false;
            self.fim.independent_identity.clear();
            self.fim.independent_version.clear();
            self.fim.reuse = None;
            self.fim.insert_before = None;
        }
        let selected = self
            .fim
            .selected_record
            .as_ref()
            .filter(|(owner, sheet, _)| owner == &key && *sheet == index)
            .map(|(_, _, record)| *record);
        let map_record = primary.and_then(|id| source.record_for_topic(id));
        let selected = selected
            .or(map_record)
            .unwrap_or_else(|| source.root_record());
        let records = source.records();
        let bound_topic = source.topic_for_record(selected);
        let metadata = bound_topic
            .and_then(|id| source.topic_metadata(id))
            .or_else(|| {
                records
                    .iter()
                    .find(|record| record.record == selected)
                    .map(|record| thinoda_core::fim::TopicMetadata {
                        record: record.record,
                        kind: record.kind,
                        identity: record.identity.clone(),
                        version: record.version.clone(),
                        cardinality: None,
                        usages: record.usages,
                    })
            });
        let dialect = match source.dialect() {
            thinoda_core::fim::Dialect::Xdf2 => Dialect::Xdf2,
            thinoda_core::fim::Dialect::Xdf3 => Dialect::Xdf3,
        };
        ui.heading(match dialect {
            Dialect::Xdf2 => "FIM · XDF2",
            Dialect::Xdf3 => "FIM · XDF3",
        });
        widgets::caption(
            ui,
            t("Save drafts as .thinoda. XDF export requires valid schemas and semantics."),
        );
        let palette = Palette::of_ui(ui);
        if let Some(metadata) = &metadata {
            let highlighted = map_record == Some(selected);
            let entry = egui::Frame::new()
                .inner_margin(6.0)
                .corner_radius(Tokens::RADIUS_S)
                .stroke(egui::Stroke::new(
                    if highlighted { 2.0 } else { 1.0 },
                    if highlighted {
                        palette.accent
                    } else {
                        palette.border
                    },
                ))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    if let Some(record) = records.iter().find(|record| record.record == selected) {
                        ui.add(egui::Label::new(egui::RichText::new(&record.name).strong()).wrap());
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(kind_label(metadata.kind));
                        if let Some(identity) = &metadata.identity {
                            ui.label(identity);
                        }
                        if let Some(version) = &metadata.version {
                            ui.label(version);
                        }
                    });
                });
            if selection_changed && highlighted {
                entry.response.scroll_to_me(None);
            }
        }
        let mut change = None;
        widgets::section(ui, t("FIM validation"), true, |ui| {
            self.schema_controls(ui, dialect);
            let semantic_errors = source
                .diagnostics()
                .iter()
                .any(|d| d.severity == DiagnosticSeverity::Error);
            if self.fim.work.is_some() {
                ui.spinner();
            } else if let Some((owner, report)) = &self.fim.report {
                let current = self.fim_owner().as_ref() == Some(owner);
                let status = if !current {
                    "Validation is stale"
                } else if report.is_exchange_conformant() && !semantic_errors {
                    "Validation passed"
                } else {
                    "Validation failed"
                };
                ui.label(t(status));
                if current {
                    for diagnostic in &report.diagnostics {
                        ui.colored_label(ui.visuals().error_fg_color, &diagnostic.message)
                            .on_hover_text(diagnostic.path.as_deref().unwrap_or_default());
                    }
                    if report.omitted_diagnostics > 0 {
                        ui.label(format!(
                            "{} + {}",
                            report.diagnostics.len(),
                            report.omitted_diagnostics
                        ));
                    }
                }
            } else {
                ui.label(t("Not checked"));
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.fim.work.is_none(),
                        egui::Button::new(t("Validate document")),
                    )
                    .clicked()
                {
                    self.start_fim_validation(&ctx, None);
                }
                if ui
                    .add_enabled(
                        self.fim.work.is_none() && !semantic_errors,
                        egui::Button::new(t("Export validated XDF")),
                    )
                    .clicked()
                {
                    self.export_fim(match dialect {
                        Dialect::Xdf2 => Format::FimXdf2,
                        Dialect::Xdf3 => Format::FimXdf3,
                    });
                }
            });
            widgets::caption(ui, t("Validation does not certify the document."));
        });
        if let Some(error) = &self.fim.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(edit) = self.doc().and_then(|doc| doc.fim_edit.clone()) {
            ui.separator();
            ui.label(t(
                "Apply before changing selection; Cancel discards this draft.",
            ));
            let mut value = change_value(&edit.change).to_owned();
            let response = ui
                .add(
                    egui::TextEdit::multiline(&mut value)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                )
                .accessible_name(t("FIM data"));
            if response.changed()
                && let Some(doc) = self.doc_mut()
            {
                let mut next = edit.change.clone();
                set_change_value(&mut next, value);
                doc.set_fim_edit(next);
            }
            ui.horizontal(|ui| {
                if ui.button(t("Apply")).clicked()
                    && let Some(doc) = self.doc_mut()
                    && let Err(error) = doc.commit_inspector_edit()
                {
                    self.fim.error = Some(error);
                }
                if ui.button(t("Cancel")).clicked()
                    && let Some(doc) = self.doc_mut()
                {
                    doc.clear_fim_edit();
                    self.fim.error = None;
                    self.fim.report = None;
                }
            });
            return;
        }
        if let Some(metadata) = &metadata {
            ui.separator();
            if metadata.usages > 1 {
                ui.label(format!("{} · {}", t("Shared definition"), metadata.usages));
                let usages: Vec<_> = self
                    .doc()
                    .into_iter()
                    .flat_map(|doc| doc.doc.sheet().topics.values())
                    .filter(|topic| {
                        source.topic_metadata(topic.id).is_some_and(|other| {
                            other.kind == metadata.kind
                                && other.identity == metadata.identity
                                && other.version == metadata.version
                        })
                    })
                    .map(|topic| (topic.id, topic.title.clone()))
                    .collect();
                ui.horizontal_wrapped(|ui| {
                    for (topic, name) in usages {
                        if ui
                            .selectable_label(
                                primary == Some(topic),
                                format!("{name} · {}", topic.0),
                            )
                            .clicked()
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.view.select_only(topic);
                        }
                    }
                });
            }
            if let Some(record) = source
                .records()
                .into_iter()
                .find(|record| record.record == metadata.record)
            {
                ui.horizontal_wrapped(|ui| {
                    ui.label(t("Name"));
                    if ui.add(egui::Button::new(&record.name).wrap()).clicked()
                        && let Some(doc) = self.doc_mut()
                    {
                        doc.set_fim_edit(FimChange::RenameDefinition {
                            record: record.record,
                            name: record.name,
                        });
                    }
                });
            }
            if let Some(cardinality) = &metadata.cardinality {
                ui.horizontal(|ui| {
                    ui.label(t("Cardinality"));
                    if ui.button(cardinality).clicked()
                        && let Some(topic) = bound_topic
                        && let Some(doc) = self.doc_mut()
                    {
                        doc.set_fim_edit(FimChange::SetCardinality {
                            topic,
                            value: cardinality.clone(),
                        });
                    }
                });
            }
            if metadata.identity.is_some()
                && let Some(topic) = bound_topic
            {
                widgets::section(ui, t("Make independent"), false, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.fim.independent_identity)
                            .hint_text(t("New identity"))
                            .desired_width(ui.available_width()),
                    )
                    .accessible_name(t("New identity"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.fim.independent_version)
                            .hint_text(t("Version"))
                            .desired_width(ui.available_width()),
                    )
                    .accessible_name(t("Version"));
                    if ui
                        .add_enabled(
                            !self.fim.independent_identity.trim().is_empty(),
                            egui::Button::new(t("Make independent")),
                        )
                        .clicked()
                    {
                        change = Some(FimChange::MakeIndependent {
                            topic,
                            identity: self.fim.independent_identity.trim().to_owned(),
                            version: if self.fim.independent_version.trim().is_empty() {
                                metadata.version.clone()
                            } else {
                                Some(self.fim.independent_version.trim().to_owned())
                            },
                        });
                    }
                });
            }
        }
        widgets::section(ui, t("Semantic diagnostics"), false, |ui| {
            for diagnostic in source.diagnostics() {
                let color = match diagnostic.severity {
                    DiagnosticSeverity::Error => ui.visuals().error_fg_color,
                    DiagnosticSeverity::Warning => ui.visuals().warn_fg_color,
                    DiagnosticSeverity::Info => ui.visuals().text_color(),
                };
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new(&diagnostic.message).color(color))
                            .wrap(),
                    )
                    .clicked()
                {
                    if let Some(topic) = diagnostic.topic
                        && let Some(doc) = self.doc_mut()
                    {
                        doc.view.select_only(topic);
                    }
                    if let Some(record) = diagnostic.record {
                        self.fim.selected_record = Some((key.clone(), index, record));
                        self.fim.page = 0;
                    }
                }
            }
        });
        widgets::section(ui, t("Add FIM element"), false, |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.fim.new_name).hint_text(t("Name")))
                .accessible_name(t("Name"));
            if let Some(parent) = bound_topic {
                ui.horizontal_wrapped(|ui| {
                    for kind in [
                        FimKind::Field,
                        FimKind::Group,
                        FimKind::Choice,
                        FimKind::CodeList,
                        FimKind::Rule,
                    ] {
                        if ui
                            .button(format!("{} · {}", kind_label(kind), kind_name(kind)))
                            .clicked()
                        {
                            change = Some(FimChange::Add {
                                parent,
                                kind,
                                name: self.fim.new_name.clone(),
                            });
                        }
                    }
                });
                egui::ComboBox::from_label(t("Shared definition"))
                    .selected_text(t("Shared definition"))
                    .show_ui(ui, |ui| {
                        for record in &records {
                            if ui
                                .selectable_label(
                                    self.fim.reuse == Some(record.record),
                                    format!("{} · {}", kind_label(record.kind), record.name),
                                )
                                .clicked()
                            {
                                self.fim.reuse = Some(record.record);
                            }
                        }
                    });
                if let Some(record) = self.fim.reuse
                    && ui.button(t("Apply")).clicked()
                {
                    change = Some(FimChange::Reuse { parent, record });
                }
                if ui.button(t("Duplicate")).clicked() {
                    change = Some(FimChange::Duplicate { topic: parent });
                }
                if ui.button(t("Remove")).clicked() {
                    change = Some(FimChange::Remove { topic: parent });
                }
            }
        });
        widgets::section(ui, t("Document metadata"), false, |ui| {
            if ui.button(t("All source data")).clicked() {
                self.fim.selected_record = Some((key.clone(), index, source.root_record()));
                self.fim.page = 0;
            }
            for record in &records {
                let mut button =
                    egui::Button::new(format!("{} · {}", kind_label(record.kind), record.name))
                        .selected(selected == record.record)
                        .wrap();
                if map_record == Some(record.record) {
                    button = button.stroke(egui::Stroke::new(2.0, palette.accent));
                }
                if ui.add(button).clicked() {
                    self.fim.selected_record = Some((key.clone(), index, record.record));
                    self.fim.page = 0;
                    if let Some(topic) = source.topic_for_record(record.record)
                        && let Some(doc) = self.doc_mut()
                    {
                        doc.view.select_only(topic);
                    }
                }
            }
        });
        widgets::section(ui, t("Dependencies"), false, |ui| {
            for dependency in source.dependencies() {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add(egui::Button::new(&dependency.identity).wrap())
                        .on_hover_text(format!("{:?}", dependency.kind))
                        .clicked()
                    {
                        self.fim.selected_record = Some((key.clone(), index, dependency.source));
                        self.fim.page = 0;
                    }
                    match &dependency.resolution {
                        DependencyResolution::Internal(targets)
                        | DependencyResolution::Ambiguous(targets)
                        | DependencyResolution::ReviewHint(targets) => {
                            for target in targets {
                                let label = records
                                    .iter()
                                    .find(|record| record.record == *target)
                                    .map_or_else(
                                        || format!("#{}", target.0),
                                        |record| record.name.clone(),
                                    );
                                if ui.add(egui::Button::new(label).wrap()).clicked() {
                                    self.fim.selected_record = Some((key.clone(), index, *target));
                                    self.fim.page = 0;
                                }
                            }
                            if matches!(dependency.resolution, DependencyResolution::Ambiguous(_)) {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    t("Ambiguous reference"),
                                );
                            }
                        }
                        DependencyResolution::External => {
                            ui.label(t("External reference"));
                        }
                        DependencyResolution::Unresolved => {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                t("Unresolved reference"),
                            );
                        }
                    }
                });
            }
        });
        widgets::section(ui, t("All source data"), true, |ui| {
            ui.checkbox(
                &mut self.fim.occurrence_only,
                t("Edit only this occurrence"),
            );
            let occurrence_only = self.fim.occurrence_only;
            if ui
                .add(egui::TextEdit::singleline(&mut self.fim.filter).hint_text(t("Search")))
                .changed()
            {
                self.fim.page = 0;
            }
            let filter = self.fim.filter.to_lowercase();
            let fields: Vec<_> = source
                .fields(selected)
                .into_iter()
                .filter(|field| {
                    filter.is_empty()
                        || field.path.to_lowercase().contains(&filter)
                        || field.value.to_lowercase().contains(&filter)
                        || field
                            .attributes
                            .iter()
                            .any(|attribute| attribute.value.to_lowercase().contains(&filter))
                })
                .collect();
            const PAGE: usize = 40;
            self.fim.page = self.fim.page.min(fields.len().saturating_sub(1) / PAGE);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.fim.page > 0, egui::Button::new("<"))
                    .accessible_name(t("Previous"))
                    .clicked()
                {
                    self.fim.page -= 1;
                }
                ui.label(format!(
                    "{} / {} · {}",
                    self.fim.page + 1,
                    fields.len().div_ceil(PAGE).max(1),
                    fields.len()
                ));
                if ui
                    .add_enabled(
                        (self.fim.page + 1) * PAGE < fields.len(),
                        egui::Button::new(">"),
                    )
                    .accessible_name(t("Next"))
                    .clicked()
                {
                    self.fim.page += 1;
                }
            });
            for field in fields.into_iter().skip(self.fim.page * PAGE).take(PAGE) {
                ui.push_id(field.record.0, |ui| {
                    egui::CollapsingHeader::new(format!("{} · #{}", field.name, field.record.0))
                        .default_open(!field.value.trim().is_empty())
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(&field.path).wrap());
                            ui.small(&field.namespace);
                            if !field.has_elements {
                                let value = if field.value.is_empty() {
                                    t("Value")
                                } else {
                                    field.value.as_str()
                                };
                                if ui.add(egui::Button::new(value).wrap()).clicked()
                                    && let Some(doc) = self.doc_mut()
                                {
                                    let change = if occurrence_only {
                                        FimChange::SetOccurrenceText {
                                            record: field.record,
                                            value: field.value.clone(),
                                        }
                                    } else {
                                        FimChange::SetText {
                                            record: field.record,
                                            value: field.value.clone(),
                                        }
                                    };
                                    doc.set_fim_edit(change);
                                }
                            }
                            for attribute in &field.attributes {
                                ui.horizontal_wrapped(|ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(format!(
                                                "@{} = {}",
                                                attribute.name, attribute.value
                                            ))
                                            .wrap(),
                                        )
                                        .clicked()
                                        && let Some(doc) = self.doc_mut()
                                    {
                                        doc.set_fim_edit(FimChange::SetAttribute {
                                            record: field.record,
                                            name: attribute.name.clone(),
                                            value: Some(attribute.value.clone()),
                                        });
                                    }
                                    if ui
                                        .small_button("×")
                                        .accessible_name(format!(
                                            "{} @{}",
                                            t("Remove"),
                                            attribute.name
                                        ))
                                        .clicked()
                                    {
                                        change = Some(FimChange::SetAttribute {
                                            record: field.record,
                                            name: attribute.name.clone(),
                                            value: None,
                                        });
                                    }
                                });
                            }
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("+").accessible_name(t("Attribute")).clicked() {
                                    self.fim.selected_record =
                                        Some((key.clone(), index, field.record));
                                }
                                if ui.button(t("Remove")).clicked() {
                                    change = Some(FimChange::RemoveElement {
                                        record: field.record,
                                    });
                                }
                            });
                        });
                });
            }
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.fim.new_element).hint_text(t("Name")))
                .accessible_name(t("Name"));
            ui.add(egui::TextEdit::singleline(&mut self.fim.new_namespace).hint_text("Namespace"))
                .accessible_name("Namespace");
            let children: Vec<_> = source
                .fields(selected)
                .into_iter()
                .filter(|field| field.parent == Some(selected))
                .collect();
            if self
                .fim
                .insert_before
                .is_some_and(|record| !children.iter().any(|field| field.record == record))
            {
                self.fim.insert_before = None;
            }
            egui::ComboBox::from_label(t("Insert before"))
                .selected_text(
                    self.fim
                        .insert_before
                        .and_then(|record| children.iter().find(|field| field.record == record))
                        .map_or_else(
                            || t("Append").to_owned(),
                            |field| format!("{}: {}", t("Insert before"), field.name),
                        ),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.fim.insert_before, None, t("Append"));
                    for field in &children {
                        ui.selectable_value(
                            &mut self.fim.insert_before,
                            Some(field.record),
                            &field.name,
                        );
                    }
                });
            if ui.button(t("Add FIM element")).clicked() {
                let namespace = if self.fim.new_namespace.is_empty() {
                    source
                        .fields(selected)
                        .first()
                        .map(|field| field.namespace.clone())
                        .unwrap_or_default()
                } else {
                    self.fim.new_namespace.clone()
                };
                change = Some(FimChange::InsertElement {
                    parent: selected,
                    before: self.fim.insert_before,
                    name: self.fim.new_element.clone(),
                    namespace,
                    text: None,
                });
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.fim.new_attribute).hint_text(t("Attribute")),
            )
            .accessible_name(t("Attribute"));
            let attribute_name = self.fim.new_attribute.clone();
            if ui.button(t("Attribute")).clicked()
                && let Some(doc) = self.doc_mut()
            {
                doc.set_fim_edit(FimChange::SetAttribute {
                    record: selected,
                    name: attribute_name,
                    value: Some(String::new()),
                });
            }
        });
        if let Some(change) = change {
            self.apply_fim(&ctx, change);
        }
    }
}

fn kind_label(kind: FimKind) -> &'static str {
    match kind {
        FimKind::Field => "F",
        FimKind::Group => "G",
        FimKind::Rule => "R",
        FimKind::CodeList => "C",
        FimKind::Schema => "S",
        FimKind::Choice => "X",
        FimKind::DocumentProfile => "D",
        FimKind::Translation => "T",
        FimKind::Collection => "B",
    }
}

fn kind_name(kind: FimKind) -> &'static str {
    match kind {
        FimKind::Field => t("Field"),
        FimKind::Group | FimKind::Choice => t("Group"),
        FimKind::Rule => t("Rule"),
        FimKind::CodeList => t("Code list"),
        _ => "FIM",
    }
}

fn change_value(change: &FimChange) -> &str {
    match change {
        FimChange::SetText { value, .. }
        | FimChange::SetOccurrenceText { value, .. }
        | FimChange::SetCardinality { value, .. } => value,
        FimChange::SetAttribute { value, .. } => value.as_deref().unwrap_or_default(),
        FimChange::RenameDefinition { name, .. } => name,
        _ => "",
    }
}

fn set_change_value(change: &mut FimChange, text: String) {
    match change {
        FimChange::SetText { value, .. }
        | FimChange::SetOccurrenceText { value, .. }
        | FimChange::SetCardinality { value, .. } => *value = text,
        FimChange::SetAttribute { value, .. } => *value = Some(text),
        FimChange::RenameDefinition { name, .. } => *name = text,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thinoda_core::{Document, Workbook};

    #[test]
    fn typing_cannot_rebind_a_fim_draft_to_a_concurrent_source_revision() {
        let mut doc = crate::doc::OpenDoc::new(Workbook::default());
        let root = doc.doc.sheet().root;
        doc.set_fim_edit(FimChange::SetCardinality {
            topic: root,
            value: "0:*".into(),
        });
        let original_state = doc.fim_edit.as_ref().unwrap().state;
        doc.doc
            .edit_sheet("Concurrent source", |sheet| {
                sheet.set_title(root, "Concurrent")
            })
            .unwrap();
        doc.set_fim_edit(FimChange::SetCardinality {
            topic: root,
            value: "1:3".into(),
        });
        assert_eq!(doc.fim_edit.as_ref().unwrap().state, original_state);
        assert!(doc.commit_inspector_edit().is_err());
        assert!(doc.fim_edit.is_some());
        assert_eq!(doc.doc.sheet().root_topic().title, "Concurrent");
    }

    #[test]
    #[ignore = "requires THINODA_FIM_REFERENCE_DIR and THINODA_FIM_SAMPLE local reference paths"]
    fn official_schema_cache_and_supplied_document_roundtrip() {
        let directory = PathBuf::from(
            std::env::var_os("THINODA_FIM_REFERENCE_DIR").expect("official reference directory"),
        );
        let sample =
            std::fs::read(std::env::var_os("THINODA_FIM_SAMPLE").expect("supplied document path"))
                .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let paths = thinoda_platform::Paths::from_root(temp.path());
        let mut schemas = FimSchemas::new(Some(&paths));
        let ctx = Context::default();
        assert!(schemas.install_paths(
            Dialect::Xdf3,
            directory.join("xdf3-official-schemas.zip"),
            Some(directory.join("xdf3-official-examples.zip")),
        ));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while schemas.busy() && std::time::Instant::now() < deadline {
            schemas.poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(schemas.status(Dialect::Xdf3), &SchemaStatus::Ready);
        let book = thinoda_io::fim::import_document(&sample).unwrap();
        let validator = schemas.validator(Dialect::Xdf3).unwrap();
        assert_eq!(
            thinoda_io::fim::export_validated(book.active(), &validator).unwrap(),
            sample
        );
        let mut doc = Document::new(book);
        let record = doc
            .sheet()
            .fim
            .as_ref()
            .unwrap()
            .records()
            .into_iter()
            .find(|record| record.kind == FimKind::Field)
            .unwrap();
        doc.edit_sheet("Rename field", |sheet| {
            sheet
                .apply_fim_change(FimChange::RenameDefinition {
                    record: record.record,
                    name: "Reviewed field name".into(),
                })
                .map_err(|error| thinoda_core::EditError::Invalid(error.to_string()))
        })
        .unwrap();
        let authored = thinoda_io::fim::export_validated(doc.sheet(), &validator).unwrap();
        assert_ne!(authored, sample);
        let native =
            thinoda_io::export_bytes(doc.workbook(), Format::Thinoda, &Default::default()).unwrap();
        let reopened: Workbook = thinoda_io::import_bytes(Format::Thinoda, &native).unwrap();
        assert_eq!(
            thinoda_io::fim::export_validated(reopened.active(), &validator).unwrap(),
            authored
        );
        doc.undo();
        assert_eq!(
            thinoda_io::fim::export_validated(doc.sheet(), &validator).unwrap(),
            sample
        );
        drop(validator);
        let mut reloaded = FimSchemas::new(Some(&paths));
        assert!(reloaded.ensure_cached(Dialect::Xdf3));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while reloaded.busy() && std::time::Instant::now() < deadline {
            reloaded.poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(reloaded.status(Dialect::Xdf3), &SchemaStatus::Ready);
        let mut app = ThinodaApp::headless();
        app.fim.schemas = reloaded;
        app.docs[0].doc = doc;
        let output = temp.path().join("validated.xdf3.xml");
        app.start_fim_validation(&ctx, Some((output.clone(), sample.clone())));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while app.fim.work.is_some() && std::time::Instant::now() < deadline {
            app.poll_fim(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(std::fs::read(&output).unwrap(), sample);
        let invalid = format!("<x:xdatenfelder.stammdatenschema.0102 xmlns:x='{}'><x:stammdatenschema><x:name>Invalid repair draft</x:name></x:stammdatenschema></x:xdatenfelder.stammdatenschema.0102>", Dialect::Xdf3.namespace()).into_bytes();
        app.docs[0].doc = Document::new(thinoda_io::fim::import_document(&invalid).unwrap());
        let rejected = temp.path().join("must-not-exist.xdf3.xml");
        app.start_fim_validation(&ctx, Some((rejected.clone(), invalid)));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while app.fim.work.is_some() && std::time::Instant::now() < deadline {
            app.poll_fim(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            !rejected.exists(),
            "invalid XDF must never reach the destination"
        );
        assert!(app.fim.error.is_some());
    }
}
