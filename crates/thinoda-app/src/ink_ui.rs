//! Host-side feedback and explicit recovery for transient ink input.

use std::fmt::Write as _;

use egui::{Align2, Context, Id, RichText, vec2};
use serde::{Deserialize, Serialize};
use thinoda_canvas::{
    ink_controller::{
        InkCancelReason, InkControllerError, InkMode, InkOutcome, InkRecoveryBlock,
        RecoveredInkGesture,
    },
    ink_input::{InkInputError, MAX_INPUT_SAMPLES},
};
use thinoda_core::board::MAX_BOARD_COORDINATE;
use thinoda_platform::recovery::MAX_PENDING_INK_BYTES;

use crate::{app::ThinodaApp, doc::OpenDoc, i18n::t};

/// Keep structured rejected samples beyond the canvas frame that reported them.
#[derive(Debug, Default)]
pub(crate) struct InkFeedback {
    errors: Vec<InkFeedbackEntry>,
    overflow: usize,
    revision: u64,
}

#[derive(Debug)]
enum InkFeedbackEntry {
    Live(InkControllerError),
    Recovered(RecoveredFeedback),
}

impl InkFeedbackEntry {
    fn details(&self) -> String {
        match self {
            Self::Live(error) => format!("{error:?}"),
            Self::Recovered(error) => error.details.clone(),
        }
    }

    fn guidance(&self) -> InkGuidance {
        match self {
            Self::Live(error) => InkGuidance::for_error(error),
            Self::Recovered(error) => error.guidance,
        }
    }

    fn retain_after_completion(&self) -> bool {
        match self {
            Self::Live(error) => contains_unaccepted_sample(error),
            Self::Recovered(error) => error.retain_after_completion,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveredFeedback {
    details: String,
    retain_after_completion: bool,
    guidance: InkGuidance,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum InkGuidance {
    PointerHeld,
    Interrupted,
    EditorBlocked,
    UncorrelatedPen,
    UncorrelatedTouch,
    Rejected,
    StaleDocument,
    CameraChanged,
    Locked,
    ClipWouldChange,
    Simplification,
    SceneDependent,
    Other,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InkRecovery {
    version: u8,
    gesture: Option<RecoveredInkGesture>,
    feedback: Vec<RecoveredFeedback>,
    overflow: usize,
    retry_tolerance_bits: u32,
    retry_smooth: bool,
}

impl InkFeedback {
    pub(crate) fn is_empty(&self) -> bool {
        self.errors.is_empty() && self.overflow == 0
    }

    pub(crate) fn record(&mut self, error: InkControllerError) {
        if self.errors.len() < MAX_INPUT_SAMPLES {
            self.errors.push(InkFeedbackEntry::Live(error));
        } else {
            self.overflow = self.overflow.saturating_add(1);
        }
        self.revision = self.revision.wrapping_add(1);
    }

    fn completed(&mut self) {
        // A later successful gesture must not erase a separate rejected start.
        let before = self.errors.len();
        self.errors
            .retain(InkFeedbackEntry::retain_after_completion);
        if self.errors.len() != before {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn clear(&mut self) {
        if !self.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        self.errors.clear();
        self.overflow = 0;
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    fn for_recovery(&self) -> Result<Vec<RecoveredFeedback>, String> {
        let mut size = 0usize;
        self.errors
            .iter()
            .map(|error| {
                let details = error.details();
                size = size.saturating_add(details.len());
                if size > MAX_PENDING_INK_BYTES {
                    return Err("pending ink feedback exceeds the recovery size limit".into());
                }
                Ok(RecoveredFeedback {
                    details,
                    retain_after_completion: error.retain_after_completion(),
                    guidance: error.guidance(),
                })
            })
            .collect()
    }
}

fn contains_unaccepted_sample(error: &InkControllerError) -> bool {
    match error {
        InkControllerError::StartWhilePending { .. }
        | InkControllerError::UncorrelatedPenInput { .. }
        | InkControllerError::UncorrelatedTouchInput { .. }
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

impl InkGuidance {
    fn for_error(error: &InkControllerError) -> Self {
        match error {
            InkControllerError::GesturePending => Self::PointerHeld,
            InkControllerError::Interrupted(_) => Self::Interrupted,
            InkControllerError::BlockedByEditor { .. } => Self::EditorBlocked,
            InkControllerError::UncorrelatedPenInput { .. } => Self::UncorrelatedPen,
            InkControllerError::UncorrelatedTouchInput { .. } => Self::UncorrelatedTouch,
            InkControllerError::InvalidSample(_)
            | InkControllerError::InvalidPathSample(_)
            | InkControllerError::TooManyPathSamples { .. }
            | InkControllerError::RejectedInputPending => Self::Rejected,
            InkControllerError::StaleDocument { .. } => Self::StaleDocument,
            InkControllerError::CameraChanged { .. } => Self::CameraChanged,
            InkControllerError::Locked(_) => Self::Locked,
            InkControllerError::ClipWouldChange(_) => Self::ClipWouldChange,
            InkControllerError::RecoveryNeedsReview => Self::SceneDependent,
            InkControllerError::Ink(
                InkInputError::TooManyPoints { .. }
                | InkInputError::SimplificationTooComplex { .. }
                | InkInputError::InvalidTolerance(_),
            ) => Self::Simplification,
            _ => Self::Other,
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::PointerHeld => "The pointer is still down. Release it before retrying.",
            Self::Interrupted => "The ink gesture was interrupted. Retry it or discard it.",
            Self::EditorBlocked => "Finish the active text editor before drawing again.",
            Self::UncorrelatedPen => {
                "Pen input could not be matched. No stroke was started; dismiss this message and try again."
            }
            Self::UncorrelatedTouch => {
                "Touch pointer input could not be matched. Check the gesture or dismiss this message."
            }
            Self::Rejected => {
                "Some ink input was rejected. This gesture cannot be retried; copy its samples or discard it."
            }
            Self::StaleDocument => {
                "The document changed during the gesture. Copy its samples or discard it."
            }
            Self::CameraChanged => {
                "The canvas changed during the gesture. Copy its samples or discard it."
            }
            Self::Locked => {
                "Ink could not be applied to a locked object. Copy the samples or discard the gesture."
            }
            Self::ClipWouldChange => {
                "This operation would change frame clipping. Copy the samples or discard the gesture."
            }
            Self::Simplification => {
                "Adjust ink simplification and retry, or copy the samples and discard the gesture."
            }
            Self::SceneDependent => {
                "Recovered eraser or lasso input cannot be retried. Copy its samples or discard it."
            }
            Self::Other => "The ink gesture could not be completed. Retry it or discard it.",
        }
    }
}

impl OpenDoc {
    pub(crate) fn pending_ink_for_recovery(
        &self,
        has_other_drafts: bool,
    ) -> Result<String, String> {
        let record = InkRecovery {
            version: 1,
            gesture: self
                .view
                .pending_ink_for_recovery(&self.doc, has_other_drafts),
            feedback: self.ink_feedback.for_recovery()?,
            overflow: self.ink_feedback.overflow,
            retry_tolerance_bits: self.view.ink_settings.finish.tolerance.to_bits(),
            retry_smooth: self.view.ink_settings.finish.smooth,
        };
        let payload = serde_json::to_string(&record).map_err(|error| error.to_string())?;
        if payload.len() > MAX_PENDING_INK_BYTES {
            return Err("pending ink exceeds the recovery size limit".into());
        }
        Ok(payload)
    }

    pub(crate) fn restore_pending_ink(&mut self, payload: &str) -> Result<(), String> {
        if payload.len() > MAX_PENDING_INK_BYTES {
            return Err("pending ink exceeds the recovery size limit".into());
        }
        let record: InkRecovery =
            serde_json::from_str(payload).map_err(|error| error.to_string())?;
        if record.version != 1
            || record.feedback.len() > MAX_INPUT_SAMPLES
            || record.feedback.iter().any(|error| error.details.is_empty())
            || (record.gesture.is_none() && record.feedback.is_empty() && record.overflow == 0)
        {
            return Err("invalid or unsupported pending ink recovery".into());
        }
        if self.has_unresolved_ink() {
            return Err("ink input is already active".into());
        }
        if let Some(gesture) = &record.gesture {
            self.view.restore_recovered_ink(&self.doc, gesture)?;
            self.view.ink_settings.finish.tolerance = f32::from_bits(record.retry_tolerance_bits);
            self.view.ink_settings.finish.smooth = record.retry_smooth;
        }
        let has_gesture = record.gesture.is_some();
        self.ink_feedback.errors = record
            .feedback
            .into_iter()
            .map(|mut error| {
                if matches!(error.guidance, InkGuidance::PointerHeld) {
                    error.guidance = if has_gesture {
                        InkGuidance::Interrupted
                    } else {
                        InkGuidance::Other
                    };
                }
                InkFeedbackEntry::Recovered(error)
            })
            .collect();
        self.ink_feedback.overflow = record.overflow;
        self.ink_feedback.revision = self.ink_feedback.revision.wrapping_add(1);
        Ok(())
    }

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
            "{}: {:?}\n{}: {:?}\n{}: {:?}\n{}\n",
            t("Pending ink gesture"),
            self.view.pending_ink_mode(),
            t("Input source"),
            self.view.pending_ink_pointer(),
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
                let _ = writeln!(text, "{}", error.details());
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
        let stale = pending && !doc.view.pending_ink_source_is_current(&doc.doc);
        let retryable = released
            && !stale
            && !doc.view.pending_ink_has_rejected_input()
            && doc.view.pending_ink_recovery_block().is_none();
        let mut copied = false;
        egui::Window::new(t("Pending ink gesture"))
            .id(Id::unique(("pending_ink", &doc.key, ctx.viewport_id())))
            .anchor(Align2::RIGHT_TOP, vec2(-16.0, 100.0))
            .default_width(380.0)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(t("Pending ink is not in the saved file. Crash recovery may miss recent samples."));
                if doc.view.pending_ink_was_recovered() {
                    ui.label(t("Recovered ink may be incomplete. Review it before retrying."));
                }
                if pending && !released {
                    ui.label(t("The pointer is still down. Release it before retrying."));
                } else if doc.view.pending_ink_has_rejected_input() {
                    ui.label(t("Some ink input was rejected. This gesture cannot be retried; copy its samples or discard it."));
                } else if stale {
                    ui.label(t("The document changed during the gesture. Copy its samples or discard it."));
                } else if let Some(reason) = doc.view.pending_ink_recovery_block() {
                    ui.label(t(match reason {
                        InkRecoveryBlock::StaleDocument => "The document changed during the gesture. Copy its samples or discard it.",
                        InkRecoveryBlock::SceneDependent => "Recovered eraser or lasso input cannot be retried. Copy its samples or discard it.",
                    }));
                }
                if let Some(error) = doc.ink_feedback.errors.last() {
                    ui.label(RichText::new(t(error.guidance().text())).strong());
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
                                            ui.monospace(error.details());
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
