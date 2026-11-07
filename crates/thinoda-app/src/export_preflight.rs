//! User consent for board information that an output format cannot preserve.
//!
//! The pending action owns its destination or presentation/print snapshot. Nothing
//! is written, downloaded, or queued until the user accepts the itemised report.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use egui::Context;
use thinoda_core::{PresentationSource, Sheet, Workbook};
use thinoda_io::{ExportLoss, ExportOptions, Format, VisualExportContext, print::PrintDocument};
use thinoda_ui::{Dialog, widgets};

use crate::{
    app::ThinodaApp,
    doc::{OpenDoc, is_save_target},
    i18n::{self, t},
};

const TITLE: &str = "Board export changes";
const INTRO: &str = "The selected format changes or omits board content. The board in memory stays unchanged; save as Thinoda to preserve it.";
const XMIND_METADATA: &str = "XMind stores a Thinoda-only copy of the complete board. Other apps may not display it and may discard it when saving. This export is not redacted.";

#[derive(Clone, Copy, Default)]
pub(crate) struct SaveConsent {
    pub board: bool,
    pub mindmanager: bool,
}

impl SaveConsent {
    pub(crate) fn for_io(self, format: Format) -> bool {
        if format == Format::DrawIo {
            self.board
        } else {
            self.mindmanager
        }
    }
}

#[derive(Clone)]
struct Owner {
    key: String,
    sheet: usize,
    revision: u64,
}

impl Owner {
    fn of(doc: &OpenDoc) -> Self {
        Self {
            key: doc.key.clone(),
            sheet: doc.doc.active_index(),
            revision: doc.doc.state_id(),
        }
    }

    fn matches(&self, doc: &OpenDoc) -> bool {
        self.key == doc.key
            && self.revision == doc.doc.state_id()
            && self.sheet < doc.doc.workbook().sheets.len()
            && !doc.has_unresolved_ink()
    }
}

enum Action {
    Export {
        format: Format,
        allow_symbol_color_loss: bool,
        allow_note_format_loss: bool,
    },
    Save {
        path: PathBuf,
        backup: bool,
        deliver: bool,
        sheet_only: bool,
        allow_mindmanager_loss: bool,
    },
    Presentation {
        sheet: Box<Sheet>,
        options: ExportOptions,
        images: bool,
        visual: VisualExportContext,
    },
    Print {
        document: Arc<PrintDocument>,
        title: String,
        save: bool,
    },
}

pub(crate) struct BoardPreflight {
    owner: Option<Owner>,
    viewport: egui::ViewportId,
    format: Format,
    losses: LossReport,
    action: Action,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LossReport {
    pub losses: Vec<ExportLoss>,
    pub flattened_viewport_slides: usize,
}

impl LossReport {
    pub(crate) fn is_empty(&self) -> bool {
        self.losses.is_empty() && self.flattened_viewport_slides == 0
    }
}

/// PowerPoint currently rasterizes viewport slides, unlike branch/zone slides.
/// Keep this app-side supplement until IO exposes it as an `ExportLoss`.
pub(crate) fn export_loss_report(
    workbook: &Workbook,
    format: Format,
    options: &ExportOptions,
) -> LossReport {
    let flattened_viewport_slides = if format == Format::PowerPoint {
        workbook
            .sheets
            .get(
                options
                    .sheet
                    .unwrap_or(workbook.active_sheet)
                    .min(workbook.sheets.len().saturating_sub(1)),
            )
            .map_or(0, |sheet| {
                sheet
                    .presentation
                    .slides
                    .iter()
                    .filter(|slide| matches!(slide.source, PresentationSource::Viewport))
                    .count()
            })
    } else {
        0
    };
    LossReport {
        losses: thinoda_io::export_losses(workbook, format, options),
        flattened_viewport_slides,
    }
}

/// Localized, separately readable rows, including counts and synthesized titles.
pub(crate) fn loss_lines(report: &LossReport, format: Format) -> Vec<String> {
    let fim = matches!(format, Format::FimXdf2 | Format::FimXdf3);
    let mut lines: Vec<_> = report
        .losses
        .iter()
        .map(|loss| {
            let (count, label) = match loss {
                ExportLoss::ShapesDropped(count) => (*count, "Shapes omitted"),
                ExportLoss::InkDropped(count) => (*count, "Ink strokes omitted"),
                ExportLoss::ConnectorsDropped(count) => (*count, "Connectors omitted"),
                ExportLoss::CardsConvertedToTopics(count) if fim => {
                    (*count, "Card presentation omitted")
                }
                ExportLoss::CardsConvertedToTopics(count) => {
                    (*count, "Cards converted to ordinary topics")
                }
                ExportLoss::CardsDropped(count) => (*count, "Cards omitted"),
                ExportLoss::RotationDropped(count) => (*count, "Card rotations omitted"),
                ExportLoss::ViewportSlidesDropped(count) => (*count, "Viewport slides omitted"),
                ExportLoss::ZonesDropped(count) => (*count, "Zones omitted"),
                ExportLoss::ZoneClipDropped(count) => (*count, "Zone clipping not fully applied"),
                ExportLoss::FloatingTopicsDropped(count) => (*count, "Floating root trees omitted"),
                ExportLoss::SummaryTreesDropped(count) => (*count, "Summary topic trees omitted"),
                ExportLoss::RelationshipsDropped(count) => (*count, "Relationships omitted"),
                ExportLoss::BoundariesDropped(count) => (*count, "Boundaries omitted"),
                ExportLoss::OrientedImageMetadataDropped(count) => (
                    *count,
                    "Camera image metadata or color profiles may be omitted",
                ),
                ExportLoss::InlineTextDropped(count) => {
                    (*count, "Inline card text and list formatting omitted")
                }
                ExportLoss::MapElementsFlattened(count) => {
                    (*count, "Mind-map topics and links flattened to shapes")
                }
                ExportLoss::ConnectorAttachmentsDetached(count) => {
                    (*count, "Connector-to-connector attachments detached")
                }
                ExportLoss::TopicDetailsDropped(count) => {
                    (*count, "Topic notes, labels, markers or tasks omitted")
                }
                ExportLoss::SlidesDropped(count) => (*count, "Presentation slides omitted"),
                ExportLoss::BoardSettingsDropped => {
                    return t("Board grid and snapping settings omitted").to_owned();
                }
                ExportLoss::SynthesizedRoot { sheet, .. } if fim => {
                    return t("Sheet {sheet}: hidden root mode omitted.")
                        .replace("{sheet}", &(sheet + 1).to_string());
                }
                ExportLoss::SynthesizedRoot { sheet, title } => {
                    // Insert the untrusted title last so it cannot introduce another
                    // substituted placeholder, and make control characters visible.
                    return t("Sheet {sheet}: a root named “{title}” will be created.")
                        .replace("{sheet}", &(sheet + 1).to_string())
                        .replace("{title}", &title.escape_debug().to_string());
                }
            };
            format!("{count} — {}", t(label))
        })
        .collect();
    if report.flattened_viewport_slides > 0 {
        lines.push(format!(
            "{} — {}",
            report.flattened_viewport_slides,
            t("Viewport slides flattened to non-editable images")
        ));
    }
    if format == Format::XMind && !report.losses.is_empty() {
        lines.push(t(XMIND_METADATA).to_owned());
    }
    lines
}

pub(crate) fn loss_message(report: &LossReport, format: Format) -> String {
    format!("{}\n{}", t(INTRO), loss_lines(report, format).join("\n"))
}

/// The lower-level app save APIs fail closed too, including background saves.
pub(crate) fn require_board_consent(
    workbook: &Workbook,
    format: Format,
    options: &ExportOptions,
    allowed: bool,
) -> Result<(), String> {
    let losses = export_loss_report(workbook, format, options);
    if losses.is_empty() || allowed {
        Ok(())
    } else {
        Err(loss_message(&losses, format))
    }
}

pub(crate) fn save_losses(
    doc: &OpenDoc,
    sheet_only: bool,
    format: Format,
) -> Result<LossReport, String> {
    doc.ensure_no_pending_ink()?;
    // Cloning a Document also clones its undo history. Most saves have no
    // outstanding drafts, so analyze the live workbook directly in that case.
    let draft = if doc.view.has_title_changes()
        || doc.view.has_board_label_changes()
        || doc.inspector_edit.is_some()
        || doc.fim_edit.is_some()
    {
        Some(doc.document_with_drafts()?)
    } else {
        None
    };
    let source = draft
        .as_ref()
        .map_or_else(|| doc.doc.workbook(), |draft| draft.workbook());
    if sheet_only {
        let sheet = Workbook {
            metadata: source.metadata.clone(),
            ..Workbook::new(source.active().clone())
        };
        Ok(export_loss_report(
            &sheet,
            format,
            &ExportOptions::default(),
        ))
    } else {
        Ok(export_loss_report(
            source,
            format,
            &ExportOptions::default(),
        ))
    }
}

impl ThinodaApp {
    pub(crate) fn request_export_preflight(
        &mut self,
        format: Format,
        allow_symbol_color_loss: bool,
        allow_note_format_loss: bool,
    ) {
        if self.dialogs.board_export.is_some() {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc
            .commit_canvas_drafts()
            .and_then(|_| doc.commit_inspector_edit())
        {
            self.toasts
                .error(format!("{}: {error}", t("Export failed")));
            return;
        }
        let owner = Owner::of(doc);
        let losses = export_loss_report(doc.doc.workbook(), format, &ExportOptions::default());
        if losses.is_empty() {
            self.perform_preflight_export(
                format,
                allow_symbol_color_loss,
                allow_note_format_loss,
                false,
            );
        } else {
            self.dialogs.board_export = Some(BoardPreflight {
                owner: Some(owner),
                viewport: self.current_viewport,
                format,
                losses,
                action: Action::Export {
                    format,
                    allow_symbol_color_loss,
                    allow_note_format_loss,
                },
            });
        }
    }

    pub(crate) fn request_save_preflight(
        &mut self,
        path: &Path,
        backup: bool,
        deliver: bool,
        sheet_only: bool,
        allow_mindmanager_loss: bool,
    ) -> bool {
        if self.dialogs.board_export.is_some() {
            return false;
        }
        let Some(format) = Format::from_path(path).filter(|format| is_save_target(*format)) else {
            return self.perform_preflight_save(
                path,
                backup,
                deliver,
                sheet_only,
                SaveConsent {
                    board: false,
                    mindmanager: allow_mindmanager_loss,
                },
            );
        };
        let Some(doc) = self.doc_mut() else {
            return false;
        };
        if !sheet_only && let Err(error) = doc.prepare_save(path) {
            self.toasts
                .error(format!("{}: {error}", t("Could not save")));
            return false;
        }
        let owner = Owner::of(doc);
        let losses = match save_losses(doc, sheet_only, format) {
            Ok(losses) => losses,
            Err(error) => {
                self.toasts
                    .error(format!("{}: {error}", t("Could not save")));
                return false;
            }
        };
        if losses.is_empty() {
            self.perform_preflight_save(
                path,
                backup,
                deliver,
                sheet_only,
                SaveConsent {
                    board: false,
                    mindmanager: allow_mindmanager_loss,
                },
            )
        } else {
            self.dialogs.board_export = Some(BoardPreflight {
                owner: Some(owner),
                viewport: self.current_viewport,
                format,
                losses,
                action: Action::Save {
                    path: path.to_path_buf(),
                    backup,
                    deliver,
                    sheet_only,
                    allow_mindmanager_loss,
                },
            });
            false
        }
    }

    fn perform_preflight_save(
        &mut self,
        path: &Path,
        backup: bool,
        deliver: bool,
        sheet_only: bool,
        consent: SaveConsent,
    ) -> bool {
        if sheet_only {
            self.save_sheet_to_confirmed(path, backup, consent.board, consent.mindmanager)
        } else {
            self.save_active_file(path, backup, deliver, consent.board, consent.mindmanager)
        }
    }

    /// Checks the exact, possibly unapplied presentation draft rather than the
    /// saved workbook. The editor remains frozen while consent is pending.
    pub(crate) fn request_pitch_export(
        &mut self,
        ctx: &Context,
        owner: (&str, usize, u64),
        sheet: Sheet,
        options: ExportOptions,
        images: bool,
    ) {
        if self.dialogs.board_export.is_some() {
            return;
        }
        let owner = Owner {
            key: owner.0.to_owned(),
            sheet: owner.1,
            revision: owner.2,
        };
        let Some(doc) = self.docs.iter().find(|doc| owner.matches(doc)) else {
            self.toasts.warning(t("The document changed. Try again."));
            return;
        };
        let visual = crate::visual_export::for_view(doc.view.accent, doc.view.dark);
        let format = if images { Format::Png } else { Format::Pdf };
        let workbook = Workbook::new(sheet.clone());
        let mut analysis_options = options.clone();
        analysis_options.sheet = None;
        let losses = export_loss_report(&workbook, format, &analysis_options);
        if losses.is_empty() {
            self.begin_pitch_export(ctx, sheet, options, images, visual);
        } else {
            self.dialogs.board_export = Some(BoardPreflight {
                owner: Some(owner),
                viewport: ctx.viewport_id(),
                format,
                losses,
                action: Action::Presentation {
                    sheet: Box::new(sheet),
                    options,
                    images,
                    visual,
                },
            });
            ctx.request_repaint();
        }
    }

    /// Print preview is a frozen workbook snapshot; use the same selected sheet
    /// and PDF capability analysis before either printing or saving its PDF.
    pub(crate) fn request_print_output(
        &mut self,
        ctx: &Context,
        workbook: &Workbook,
        sheet: usize,
        document: Arc<PrintDocument>,
        title: String,
        save: bool,
    ) {
        if self.dialogs.board_export.is_some() {
            return;
        }
        let options = ExportOptions {
            sheet: Some(sheet),
            ..Default::default()
        };
        let losses = export_loss_report(workbook, Format::Pdf, &options);
        if losses.is_empty() {
            self.begin_print_output(ctx, document, title, save);
        } else {
            self.dialogs.board_export = Some(BoardPreflight {
                owner: None,
                viewport: ctx.viewport_id(),
                format: Format::Pdf,
                losses,
                action: Action::Print {
                    document,
                    title,
                    save,
                },
            });
            ctx.request_repaint();
        }
    }

    pub(crate) fn board_save_pending_for(&self, key: &str) -> bool {
        self.dialogs.board_export.as_ref().is_some_and(|pending| {
            matches!(
                &pending.action,
                Action::Save {
                    sheet_only: false,
                    ..
                }
            ) && pending.owner.as_ref().is_some_and(|owner| owner.key == key)
        })
    }

    pub(crate) fn return_board_preflight_viewport(&mut self, viewport: egui::ViewportId) {
        if let Some(pending) = &mut self.dialogs.board_export
            && pending.viewport == viewport
        {
            pending.viewport = egui::ViewportId::ROOT;
        }
    }

    pub(crate) fn cancel_board_preflight_viewport(&mut self, ctx: &Context) {
        self.cancel_board_preflight_in_viewport(ctx, ctx.viewport_id());
    }

    pub(crate) fn cancel_board_preflight_in_viewport(
        &mut self,
        ctx: &Context,
        viewport: egui::ViewportId,
    ) {
        if self
            .dialogs
            .board_export
            .as_ref()
            .is_some_and(|pending| pending.viewport == viewport)
        {
            let pending = self.dialogs.board_export.take().expect("pending preflight");
            self.finish_board_preflight(ctx, pending, false);
        }
    }

    fn board_preflight_valid(&self, pending: &BoardPreflight, check_losses: bool) -> bool {
        let Some(owner) = &pending.owner else {
            return true;
        };
        let Some(doc) = self.docs.iter().find(|doc| owner.matches(doc)) else {
            return false;
        };
        if !matches!(&pending.action, Action::Presentation { .. })
            && owner.sheet != doc.doc.active_index()
        {
            return false;
        }
        if !check_losses {
            return !matches!(&pending.action, Action::Export { .. })
                || self.doc().is_some_and(|active| active.key == owner.key);
        }
        match &pending.action {
            Action::Export { format, .. } => {
                self.doc().is_some_and(|active| active.key == owner.key)
                    && !doc.view.has_title_changes()
                    && !doc.view.has_board_label_changes()
                    && doc.inspector_edit.is_none()
                    && doc.fim_edit.is_none()
                    && export_loss_report(doc.doc.workbook(), *format, &ExportOptions::default())
                        == pending.losses
            }
            Action::Save { sheet_only, .. } => save_losses(doc, *sheet_only, pending.format)
                .is_ok_and(|losses| losses == pending.losses),
            Action::Presentation { .. } => true,
            Action::Print { .. } => true,
        }
    }

    fn finish_board_preflight(&mut self, ctx: &Context, pending: BoardPreflight, accepted: bool) {
        if let Action::Save {
            path,
            backup,
            deliver,
            sheet_only,
            allow_mindmanager_loss,
        } = pending.action
        {
            let Some(owner) = pending.owner else { return };
            let index = self.docs.iter().position(|doc| doc.key == owner.key);
            let saved = if accepted {
                if let Some(index) = index {
                    self.active = index;
                    self.perform_preflight_save(
                        &path,
                        backup,
                        deliver,
                        sheet_only,
                        SaveConsent {
                            board: true,
                            mindmanager: allow_mindmanager_loss,
                        },
                    )
                } else {
                    false
                }
            } else {
                if let Some(index) = index {
                    self.docs[index].ignore_conflict = false;
                }
                false
            };
            if !self.resolve_save_all(ctx, &owner.key, saved) && !sheet_only {
                self.resolve_save_close_tab(&owner.key, saved);
            }
            return;
        }
        if !accepted {
            return;
        }
        match pending.action {
            Action::Export {
                format,
                allow_symbol_color_loss,
                allow_note_format_loss,
            } => self.perform_preflight_export(
                format,
                allow_symbol_color_loss,
                allow_note_format_loss,
                true,
            ),
            Action::Presentation {
                sheet,
                options,
                images,
                visual,
            } => self.begin_pitch_export(ctx, *sheet, options, images, visual),
            Action::Print {
                document,
                title,
                save,
            } => self.begin_print_output(ctx, document, title, save),
            Action::Save { .. } => unreachable!(),
        }
    }

    /// Returns true while a preflight owns the dialog layer, including on a
    /// different viewport. Escape, the close control, and backdrop all cancel.
    pub(crate) fn board_preflight_ui(&mut self, ctx: &Context) -> bool {
        let Some(pending) = self.dialogs.board_export.as_ref() else {
            return false;
        };
        if pending.viewport != ctx.viewport_id() {
            return true;
        }
        if !self.board_preflight_valid(pending, false) {
            let pending = self.dialogs.board_export.take().expect("pending preflight");
            self.finish_board_preflight(ctx, pending, false);
            self.toasts.warning(t("The document changed. Try again."));
            return true;
        }
        let lines = loss_lines(&pending.losses, pending.format);
        let format = pending.format;
        let mut accepted = false;
        let mut canceled = false;
        let touch = widgets::touch_activation_allowed(ctx);
        let loss_height = (ctx.content_rect().height() - 230.0).clamp(60.0, 360.0);
        let response = Dialog::new(egui::Id::unique("board_export_preflight"), t(TITLE))
            .labels(&i18n::dialog_labels())
            .modal(true)
            .close_on_backdrop(true)
            .default_width(560.0)
            // Reserve room for the scrolling report and its actions immediately,
            // including when this dialog follows another confirmation.
            .default_height((loss_height + 170.0).min(Dialog::bounds(ctx).height()))
            .scroll(false)
            .show(ctx, |ui| {
                ui.strong(t(format.label()));
                ui.label(t(INTRO));
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .id_salt("board_export_losses")
                    .max_height(loss_height)
                    .show(ui, |ui| {
                        for line in &lines {
                            ui.label(line);
                        }
                    });
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    accepted = widgets::primary_button(ui, t("Confirm")).clicked() && touch;
                    canceled = widgets::secondary_button(ui, t("Cancel")).clicked() && touch;
                });
            });
        if (accepted || canceled || response.close_requested) && touch {
            let pending = self.dialogs.board_export.take().expect("pending preflight");
            let valid = !accepted || self.board_preflight_valid(&pending, true);
            if !valid {
                self.toasts.warning(t("The document changed. Try again."));
            }
            self.finish_board_preflight(ctx, pending, accepted && valid);
            ctx.request_repaint();
        }
        true
    }
}

#[cfg(test)]
#[path = "export_preflight_tests.rs"]
mod tests;
