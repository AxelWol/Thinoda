//! Host-side feedback and explicit recovery for transient ink input.

use std::fmt::Write as _;

use egui::{Align2, Context, Id, RichText, vec2};
use thinoda_canvas::{
    ink_controller::{InkCancelReason, InkControllerError, InkMode, InkOutcome},
    ink_input::{InkInputError, MAX_INPUT_SAMPLES},
};
use thinoda_core::board::MAX_BOARD_COORDINATE;

use crate::{app::ThinodaApp, doc::OpenDoc, i18n::t};

/// Keep structured rejected samples beyond the canvas frame that reported them.
#[derive(Debug, Default)]
pub(crate) struct InkFeedback {
    errors: Vec<InkControllerError>,
    overflow: usize,
}

impl InkFeedback {
    pub(crate) fn is_empty(&self) -> bool {
        self.errors.is_empty() && self.overflow == 0
    }

    pub(crate) fn record(&mut self, error: InkControllerError) {
        if self.errors.len() < MAX_INPUT_SAMPLES {
            self.errors.push(error);
        } else {
            self.overflow = self.overflow.saturating_add(1);
        }
    }

    fn completed(&mut self) {
        // A later successful gesture must not erase a separate rejected start.
        self.errors.retain(contains_unaccepted_sample);
    }

    fn clear(&mut self) {
        self.errors.clear();
        self.overflow = 0;
    }
}

fn contains_unaccepted_sample(error: &InkControllerError) -> bool {
    match error {
        InkControllerError::StartWhilePending { .. }
        | InkControllerError::BlockedByEditor { .. }
        | InkControllerError::InvalidSettings { .. }
        | InkControllerError::InvalidTransform { .. }
        | InkControllerError::InvalidSample(_)
        | InkControllerError::InvalidPathSample(_)
        | InkControllerError::TooManyPathSamples { .. } => true,
        InkControllerError::StaleDocument { sample }
        | InkControllerError::CameraChanged { sample } => sample.is_some(),
        _ => false,
    }
}

fn guidance(error: &InkControllerError) -> &'static str {
    match error {
        InkControllerError::GesturePending => {
            "The pointer is still down. Release it before retrying."
        }
        InkControllerError::Interrupted(_) => {
            "The ink gesture was interrupted. Retry it or discard it."
        }
        InkControllerError::BlockedByEditor { .. } => {
            "Finish the active text editor before drawing again."
        }
        InkControllerError::InvalidSample(_)
        | InkControllerError::InvalidPathSample(_)
        | InkControllerError::TooManyPathSamples { .. }
        | InkControllerError::RejectedInputPending => {
            "Some ink input was rejected. This gesture cannot be retried; copy its samples or discard it."
        }
        InkControllerError::StaleDocument { .. } => {
            "The document changed during the gesture. Copy its samples or discard it."
        }
        InkControllerError::CameraChanged { .. } => {
            "The canvas changed during the gesture. Copy its samples or discard it."
        }
        InkControllerError::Locked(_) => {
            "Ink could not be applied to a locked object. Copy the samples or discard the gesture."
        }
        InkControllerError::ClipWouldChange(_) => {
            "This operation would change frame clipping. Copy the samples or discard the gesture."
        }
        InkControllerError::Ink(
            InkInputError::TooManyPoints { .. }
            | InkInputError::SimplificationTooComplex { .. }
            | InkInputError::InvalidTolerance(_),
        ) => "Adjust ink simplification and retry, or copy the samples and discard the gesture.",
        _ => "The ink gesture could not be completed. Retry it or discard it.",
    }
}

impl OpenDoc {
    pub(crate) fn record_ink_output(
        &mut self,
        outcomes: &[InkOutcome],
        errors: &[InkControllerError],
    ) {
        if outcomes
            .iter()
            .any(|outcome| matches!(outcome, InkOutcome::Canceled(InkCancelReason::Escape)))
        {
            self.ink_feedback.clear();
        } else if outcomes
            .iter()
            .any(|outcome| !matches!(outcome, InkOutcome::Canceled(_)))
            && !self.view.has_pending_ink_gesture()
        {
            self.ink_feedback.completed();
        }
        for error in errors {
            self.ink_feedback.record(error.clone());
        }
    }

    pub(crate) fn interrupt_ink(&mut self, reason: InkCancelReason) {
        if self.view.interrupt_ink() {
            self.ink_feedback
                .record(InkControllerError::Interrupted(reason));
        }
    }

    pub(crate) fn suspend_ink(&mut self, ctx: &Context) {
        if self.view.needs_ink_input_tracking() {
            let output = self.view.suspend_ink(&self.doc, ctx);
            self.record_ink_output(&output.outcomes, &output.errors);
        }
    }

    pub(crate) fn retry_pending_ink(&mut self) -> Result<(), InkControllerError> {
        let result = if matches!(
            self.view.pending_ink_mode(),
            Some(InkMode::Pen | InkMode::Highlighter)
        ) {
            self.view
                .retry_ink_with_finish_options(&mut self.doc, self.view.ink_settings.finish)
        } else {
            self.view.retry_ink(&mut self.doc)
        };
        match result {
            Ok(_) => {
                self.ink_feedback.completed();
                Ok(())
            }
            Err(error) => {
                self.ink_feedback.record(error.clone());
                Err(error)
            }
        }
    }

    pub(crate) fn discard_pending_ink(&mut self) {
        self.view.cancel_ink();
        self.ink_feedback.clear();
    }

    fn ink_samples_text(&self) -> String {
        let mut text = format!(
            "{}: {:?}\n{}: {:?}\n{}\n",
            t("Pending ink gesture"),
            self.view.pending_ink_mode(),
            t("Ink settings"),
            self.view.pending_ink_settings(),
            t("Accepted samples")
        );
        for sample in self.view.pending_ink_samples() {
            let pressure = sample
                .pressure
                .map_or_else(|| t("No pressure").to_owned(), |value| value.to_string());
            let _ = writeln!(
                text,
                "{}: ({}, {}), {}: {pressure}",
                t("Sample position"),
                sample.position.x,
                sample.position.y,
                t("Pressure")
            );
        }
        if !self.ink_feedback.is_empty() {
            let _ = writeln!(text, "{}", t("Rejected input"));
            for error in &self.ink_feedback.errors {
                let _ = writeln!(text, "{error:?}");
            }
            if self.ink_feedback.overflow > 0 {
                let _ = writeln!(
                    text,
                    "{} ({})",
                    t("Additional rejected input details could not be retained."),
                    self.ink_feedback.overflow
                );
            }
        }
        text
    }
}

impl ThinodaApp {
    /// Persistent, keyboard-accessible recovery controls for the active document.
    pub(crate) fn ink_recovery_ui(&mut self, ctx: &Context) {
        let Some(doc) = self.doc_mut() else { return };
        if !doc.has_unresolved_ink() {
            return;
        }
        let pending = doc.view.has_pending_ink_gesture();
        let released = doc.view.has_released_ink_gesture();
        let retryable = released && !doc.view.pending_ink_has_rejected_input();
        let mut copied = false;
        egui::Window::new(t("Pending ink gesture"))
            .id(Id::unique(("pending_ink", &doc.key, ctx.viewport_id())))
            .anchor(Align2::RIGHT_TOP, vec2(-16.0, 100.0))
            .default_width(380.0)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(t("Ink is not saved or included in crash recovery until it is finished."));
                if pending && !released {
                    ui.label(t("The pointer is still down. Release it before retrying."));
                } else if doc.view.pending_ink_has_rejected_input() {
                    ui.label(t("Some ink input was rejected. This gesture cannot be retried; copy its samples or discard it."));
                }
                if let Some(error) = doc.ink_feedback.errors.last() {
                    ui.label(RichText::new(t(guidance(error))).strong());
                }
                if doc.ink_feedback.overflow > 0 {
                    ui.label(format!(
                        "{} ({})",
                        t("Additional rejected input details could not be retained."),
                        doc.ink_feedback.overflow
                    ));
                }
                if pending
                    && matches!(
                        doc.view.pending_ink_mode(),
                        Some(InkMode::Pen | InkMode::Highlighter)
                    )
                {
                    ui.horizontal(|ui| {
                        ui.label(t("Simplification tolerance"));
                        ui.add(
                            egui::DragValue::new(&mut doc.view.ink_settings.finish.tolerance)
                                .range(0.0..=MAX_BOARD_COORDINATE),
                        )
                        .accessible_name(t("Simplification tolerance"));
                    });
                }
                if !doc.ink_feedback.errors.is_empty() {
                    egui::CollapsingHeader::new(t("Ink input details"))
                        .id_salt("ink_details")
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(150.0)
                                .show_rows(
                                    ui,
                                    ui.text_style_height(&egui::TextStyle::Monospace),
                                    doc.ink_feedback.errors.len(),
                                    |ui, rows| {
                                        for error in &doc.ink_feedback.errors[rows] {
                                            ui.monospace(format!("{error:?}"));
                                        }
                                    },
                                );
                        });
                }
                ui.horizontal_wrapped(|ui| {
                    if pending
                        && ui
                            .add_enabled(retryable, egui::Button::new(t("Retry ink")))
                            .clicked()
                    {
                        let _ = doc.retry_pending_ink();
                    }
                    if ui.button(t("Copy ink samples")).clicked() {
                        ctx.copy_text(doc.ink_samples_text());
                        copied = true;
                    }
                    let discard = if pending {
                        "Discard ink"
                    } else {
                        "Dismiss ink feedback"
                    };
                    if ui.button(t(discard)).clicked() {
                        doc.discard_pending_ink();
                    }
                });
            });
        if copied {
            self.toasts.info(t("Ink samples copied"));
        }
    }
}
