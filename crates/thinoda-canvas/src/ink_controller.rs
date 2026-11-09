//! Canvas-side ink gestures. The host routes owned pointer events here before
//! other canvas interactions and paints the preview above the scene.

use std::collections::{HashMap, HashSet};
use std::fmt;

use egui::{Color32, Event, Key, Painter, PointerButton, Pos2, Rect, Shape, Stroke, TouchPhase};
use serde::{Deserialize, Serialize};
use thinoda_core::board::{
    BoardMember, MAX_BOARD_COORDINATE, MAX_CANVAS_OBJECTS, MAX_TOTAL_INK_POINTS, points_bounds,
};
use thinoda_core::scene::{SceneGeometry, object_clip_rect};
use thinoda_core::{
    Band, CanvasObject, CanvasObjectId, CanvasObjectKind, Color, Document, EditError, InkStroke,
    InkTool, Scene, SceneHit, SceneId, Sheet, Vec2,
};

use crate::ink_input::{
    GeometryError, InkInput, InkInputError, InkSample, MAX_GEOMETRY_CHECKS, MAX_INPUT_SAMPLES,
    PixelEraseError, PixelEraseOptions, PixelEraseResult, RejectedSample, StrokeFinishOptions,
    eraser_hits, lasso_hits, pixel_erase_stroke,
};
use crate::pen_input::{PenFrameData, PenInputIssue, PenSample};
use crate::render::{Xf, color32};
use crate::touch_input::TouchPointerIssue;

/// Ink modes are separate from the board's shape/selection tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InkMode {
    Pen,
    Highlighter,
    ObjectEraser,
    PixelEraser,
    Lasso,
}

/// Style and processing settings are captured when a gesture begins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkSettings {
    pub color: Color,
    pub width: f32,
    pub opacity: f32,
    pub band: Band,
    /// Radius in sheet units, not screen pixels.
    pub eraser_radius: f32,
    pub finish: StrokeFinishOptions,
}

impl Default for InkSettings {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            width: 2.0,
            opacity: 1.0,
            band: Band::Above,
            eraser_radius: 10.0,
            finish: StrokeFinishOptions::default(),
        }
    }
}

/// Process-independent description of the pointer that owned a captured gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveredPointer {
    Mouse,
    Pen { id: i32 },
    Touch { device: u64, id: u64 },
}

/// Float bit patterns avoid JSON's loss of non-finite values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveredInkSample {
    pub x_bits: u32,
    pub y_bits: u32,
    pub pressure_bits: Option<u32>,
}

impl From<InkSample> for RecoveredInkSample {
    fn from(sample: InkSample) -> Self {
        Self {
            x_bits: sample.position.x.to_bits(),
            y_bits: sample.position.y.to_bits(),
            pressure_bits: sample.pressure.map(f32::to_bits),
        }
    }
}

impl From<RecoveredInkSample> for InkSample {
    fn from(sample: RecoveredInkSample) -> Self {
        Self {
            position: Vec2::new(f32::from_bits(sample.x_bits), f32::from_bits(sample.y_bits)),
            pressure: sample.pressure_bits.map(f32::from_bits),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveredInkSettings {
    pub color: Color,
    pub width_bits: u32,
    pub opacity_bits: u32,
    pub band: Band,
    pub eraser_radius_bits: u32,
    pub tolerance_bits: u32,
    pub smooth: bool,
}

impl From<InkSettings> for RecoveredInkSettings {
    fn from(settings: InkSettings) -> Self {
        Self {
            color: settings.color,
            width_bits: settings.width.to_bits(),
            opacity_bits: settings.opacity.to_bits(),
            band: settings.band,
            eraser_radius_bits: settings.eraser_radius.to_bits(),
            tolerance_bits: settings.finish.tolerance.to_bits(),
            smooth: settings.finish.smooth,
        }
    }
}

impl From<RecoveredInkSettings> for InkSettings {
    fn from(settings: RecoveredInkSettings) -> Self {
        Self {
            color: settings.color,
            width: f32::from_bits(settings.width_bits),
            opacity: f32::from_bits(settings.opacity_bits),
            band: settings.band,
            eraser_radius: f32::from_bits(settings.eraser_radius_bits),
            finish: StrokeFinishOptions {
                tolerance: f32::from_bits(settings.tolerance_bits),
                smooth: settings.smooth,
            },
        }
    }
}

/// Accepted input and its original ownership, without process-local addresses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveredInkGesture {
    pub mode: InkMode,
    pub settings: RecoveredInkSettings,
    pub pointer: RecoveredPointer,
    pub sheet: usize,
    pub was_held: bool,
    pub rejected: bool,
    pub source_current: bool,
    pub additive: bool,
    pub samples: Vec<RecoveredInkSample>,
}

/// Reasons a recovered gesture must remain available for review only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InkRecoveryBlock {
    StaleDocument,
    SceneDependent,
}

/// Events and ownership supplied by the containing canvas for one frame.
pub struct InkFrameInput<'a> {
    pub events: &'a [Event],
    pub canvas: Rect,
    pub transform: Xf,
    /// False when the canvas loses ownership, becomes disabled, or a popup opens.
    pub enabled: bool,
    /// Whether a new gesture may start; active gestures continue outside the rect.
    pub can_start: bool,
    /// Modifier state for touch input.
    pub modifiers: egui::Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InkCancelReason {
    ToolChanged,
    Disabled,
    Escape,
    FocusLost,
    PointerGone,
    PointerSourceChanged,
    TouchCanceled,
    Multitouch,
}

/// Apply lasso hits to the host's shared topic/object selection. Locked items
/// remain selectable, but edit operations must still enforce locks.
#[derive(Debug, Clone, PartialEq)]
pub struct InkSelection {
    pub hits: Vec<SceneHit>,
    pub additive: bool,
}

/// Identity mapping for one pixel-erased stroke, in paint order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InkReplacement {
    pub original: CanvasObjectId,
    /// Empty when the entire stroke was erased.
    pub fragments: Vec<CanvasObjectId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InkOutcome {
    Drawn(CanvasObjectId),
    ObjectErased(Vec<CanvasObjectId>),
    PixelErased {
        originals: Vec<CanvasObjectId>,
        /// Includes retained original IDs and any newly allocated fragment IDs.
        fragments: Vec<CanvasObjectId>,
        /// Identifies the fragments belonging to each original stroke.
        replacements: Vec<InkReplacement>,
    },
    Selected(InkSelection),
    Unchanged,
    Canceled(InkCancelReason),
}

#[derive(Debug, Clone, PartialEq)]
pub enum InkControllerError {
    GesturePending,
    /// Input ownership was interrupted; the samples are retained for review.
    Interrupted(InkCancelReason),
    UncorrelatedPenInput {
        issue: PenInputIssue,
        position: Pos2,
    },
    UncorrelatedTouchInput {
        issue: TouchPointerIssue,
        position: Pos2,
    },
    StartWhilePending {
        position: Pos2,
        pressure: Option<f32>,
    },
    BlockedByEditor {
        position: Pos2,
        pressure: Option<f32>,
        error: EditError,
    },
    NoGesture,
    NotDrawingGesture,
    InvalidSettings {
        reason: String,
        sample: InkSample,
    },
    InvalidTransform {
        position: Pos2,
        pressure: Option<f32>,
    },
    InvalidSample(RejectedSample),
    InvalidPathSample(InkSample),
    TooManyPathSamples {
        sample: InkSample,
        limit: usize,
    },
    RejectedInputPending,
    RecoveryNeedsReview,
    StaleDocument {
        sample: Option<InkSample>,
    },
    CameraChanged {
        sample: Option<InkSample>,
    },
    StaleScene(CanvasObjectId),
    StaleSelection(SceneHit),
    Locked(Vec<BoardMember>),
    ClipWouldChange(CanvasObjectId),
    Limit {
        what: &'static str,
        limit: usize,
    },
    Ink(InkInputError),
    Geometry(GeometryError),
    Pixel {
        object: CanvasObjectId,
        error: PixelEraseError,
    },
    Edit(EditError),
}

impl fmt::Display for InkControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GesturePending => write!(f, "finish or cancel the pending ink gesture"),
            Self::Interrupted(reason) => {
                write!(
                    f,
                    "ink gesture interrupted ({reason:?}); retry or discard it"
                )
            }
            Self::UncorrelatedPenInput { issue, position } => write!(
                f,
                "pen input at {position:?} could not be correlated ({issue:?}); no gesture was started"
            ),
            Self::UncorrelatedTouchInput { issue, position } => write!(
                f,
                "touch pointer input at {position:?} could not be correlated ({issue:?})"
            ),
            Self::StartWhilePending { .. } => {
                write!(
                    f,
                    "finish or cancel the pending ink gesture before starting another"
                )
            }
            Self::BlockedByEditor { error, .. } => {
                write!(f, "finish the current editor before starting ink: {error}")
            }
            Self::NoGesture => write!(f, "there is no pending ink gesture"),
            Self::NotDrawingGesture => write!(f, "the pending gesture is not an ink stroke"),
            Self::InvalidSettings { reason, .. } => write!(f, "invalid ink settings: {reason}"),
            Self::InvalidTransform { .. } => write!(f, "invalid canvas transform"),
            Self::InvalidSample(error) => write!(f, "{error}"),
            Self::InvalidPathSample(_) => {
                write!(f, "gesture sample position or pressure is invalid")
            }
            Self::TooManyPathSamples { limit, .. } => write!(f, "gesture exceeds {limit} samples"),
            Self::RejectedInputPending => write!(f, "gesture contains rejected input; cancel it"),
            Self::RecoveryNeedsReview => {
                write!(
                    f,
                    "the recovered gesture needs its original scene and selection"
                )
            }
            Self::StaleDocument { .. } => write!(f, "the document changed during the gesture"),
            Self::CameraChanged { .. } => write!(f, "the camera changed during the gesture"),
            Self::StaleScene(id) => write!(f, "the scene is stale for ink object {id}"),
            Self::StaleSelection(hit) => write!(
                f,
                "the scene contains an unavailable selection target {hit:?}"
            ),
            Self::Locked(members) => write!(f, "cannot erase locked board members: {members:?}"),
            Self::ClipWouldChange(id) => write!(
                f,
                "splitting ink object {id} would change its frame clipping"
            ),
            Self::Limit { what, limit } => write!(f, "{what} exceeds the limit of {limit}"),
            Self::Ink(error) => write!(f, "{error}"),
            Self::Geometry(error) => write!(f, "{error}"),
            Self::Pixel { object, error } => write!(f, "cannot split ink object {object}: {error}"),
            Self::Edit(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for InkControllerError {}

/// All events in the frame are processed; errors never silently commit a
/// partially sampled gesture. A failed gesture remains available for preview
/// until explicitly canceled (or retried, if the error is recoverable).
#[derive(Debug, Default)]
pub struct InkFrameOutput {
    /// Suppress the canvas's normal pointer handling for this frame.
    pub captured: bool,
    pub outcomes: Vec<InkOutcome>,
    pub errors: Vec<InkControllerError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TouchKey(egui::TouchDeviceId, egui::TouchId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PointerOwner {
    Mouse,
    Pen(i32),
    Touch(TouchKey),
}

fn pointer_owner(pen: Option<PenSample>) -> PointerOwner {
    pen.map_or(PointerOwner::Mouse, |sample| PointerOwner::Pen(sample.id))
}

#[derive(Debug, Clone, Copy)]
struct PointerSample {
    owner: PointerOwner,
    position: Pos2,
    pressure: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
struct DocumentStamp {
    state: u64,
    sheet: usize,
    document: u64,
    identity: usize,
}

impl DocumentStamp {
    fn capture(document: &Document) -> Self {
        Self {
            state: document.state_id(),
            sheet: document.active_index(),
            document: document.instance_id(),
            identity: std::ptr::from_ref(document.sheet()) as usize,
        }
    }

    fn matches(self, document: &Document) -> bool {
        self.state == document.state_id()
            && self.sheet == document.active_index()
            && self.document == document.instance_id()
            && self.identity == std::ptr::from_ref(document.sheet()) as usize
    }
}

enum GestureBuffer {
    Draw(InkInput),
    Path(Vec<InkSample>),
}

struct Gesture {
    mode: InkMode,
    settings: InkSettings,
    owner: PointerOwner,
    stamp: DocumentStamp,
    transform: Xf,
    canvas: Rect,
    additive: bool,
    held: bool,
    rejected: bool,
    recovery_origin: Option<(RecoveredPointer, bool)>,
    recovery_block: Option<InkRecoveryBlock>,
    preview_sheet: usize,
    buffer: GestureBuffer,
}

impl Gesture {
    fn samples(&self) -> &[InkSample] {
        match &self.buffer {
            GestureBuffer::Draw(input) => input.samples(),
            GestureBuffer::Path(samples) => samples,
        }
    }

    fn push(&mut self, sample: InkSample) -> Result<(), InkControllerError> {
        let result = match &mut self.buffer {
            GestureBuffer::Draw(input) => input
                .push(sample)
                .map_err(InkControllerError::InvalidSample),
            GestureBuffer::Path(samples) => {
                if !valid_path_sample(sample) {
                    Err(InkControllerError::InvalidPathSample(sample))
                } else if samples.len() >= MAX_INPUT_SAMPLES {
                    Err(InkControllerError::TooManyPathSamples {
                        sample,
                        limit: MAX_INPUT_SAMPLES,
                    })
                } else {
                    samples.push(sample);
                    Ok(())
                }
            }
        };
        if result.is_err() {
            self.rejected = true;
        }
        result
    }

    fn ensure_retryable(&self, document: &Document) -> Result<(), InkControllerError> {
        if self.held {
            return Err(InkControllerError::GesturePending);
        }
        if self.rejected {
            return Err(InkControllerError::RejectedInputPending);
        }
        match self.recovery_block {
            Some(InkRecoveryBlock::StaleDocument) => {
                return Err(InkControllerError::StaleDocument { sample: None });
            }
            Some(InkRecoveryBlock::SceneDependent) => {
                return Err(InkControllerError::RecoveryNeedsReview);
            }
            None => {}
        }
        if !self.stamp.matches(document) {
            return Err(InkControllerError::StaleDocument { sample: None });
        }
        Ok(())
    }
}

/// Retained outside `Document`; one successful release is one undoable edit.
#[derive(Default)]
pub struct InkController {
    gesture: Option<Gesture>,
    recovery_revision: u64,
    touches: Vec<TouchKey>,
    block_touch: bool,
    pen_touch_lock: bool,
    suppress_mouse: bool,
    mouse_down: bool,
    // Touch de-duplication alone does not mean ink owns the mouse press.
    mouse_owned: bool,
}

impl InkController {
    pub fn has_pending_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    /// A released gesture awaiting retry or cancellation, rather than a held pointer.
    pub fn has_released_gesture(&self) -> bool {
        self.gesture.as_ref().is_some_and(|gesture| !gesture.held)
    }

    pub fn pending_mode(&self) -> Option<InkMode> {
        self.gesture.as_ref().map(|gesture| gesture.mode)
    }

    pub fn pending_settings(&self) -> Option<InkSettings> {
        self.gesture.as_ref().map(|gesture| gesture.settings)
    }

    /// The pointer identity recognized by the host for the retained gesture.
    pub fn pending_pointer(&self) -> Option<RecoveredPointer> {
        self.gesture.as_ref().map(|gesture| {
            gesture.recovery_origin.map_or_else(
                || match gesture.owner {
                    PointerOwner::Mouse => RecoveredPointer::Mouse,
                    PointerOwner::Pen(id) => RecoveredPointer::Pen { id },
                    PointerOwner::Touch(TouchKey(device, id)) => RecoveredPointer::Touch {
                        device: device.0,
                        id: id.0,
                    },
                },
                |(pointer, _)| pointer,
            )
        })
    }

    pub fn pending_samples(&self) -> &[InkSample] {
        self.gesture.as_ref().map_or(&[], Gesture::samples)
    }

    /// Whether rejected input prevents retrying the pending gesture.
    pub fn pending_has_rejected_input(&self) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|gesture| gesture.rejected)
    }

    /// Monotonic in-process revision of the retained gesture.
    pub fn recovery_revision(&self) -> u64 {
        self.recovery_revision
    }

    pub fn pending_recovery_block(&self) -> Option<InkRecoveryBlock> {
        self.gesture
            .as_ref()
            .and_then(|gesture| gesture.recovery_block)
    }

    pub fn pending_was_recovered(&self) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|gesture| gesture.recovery_origin.is_some())
    }

    pub fn pending_preview_sheet(&self) -> Option<usize> {
        self.gesture.as_ref().map(|gesture| gesture.preview_sheet)
    }

    pub fn pending_source_is_current(&self, document: &Document) -> bool {
        self.gesture.as_ref().is_some_and(|gesture| {
            gesture.recovery_block != Some(InkRecoveryBlock::StaleDocument)
                && gesture.stamp.matches(document)
        })
    }

    /// Capture accepted samples and settings without reviving the original pointer.
    pub fn pending_for_recovery(
        &self,
        document: &Document,
        has_other_drafts: bool,
    ) -> Option<RecoveredInkGesture> {
        let gesture = self.gesture.as_ref()?;
        let pointer = self.pending_pointer()?;
        let was_held = gesture
            .recovery_origin
            .map_or(gesture.held, |(_, held)| held);
        Some(RecoveredInkGesture {
            mode: gesture.mode,
            settings: gesture.settings.into(),
            pointer,
            sheet: gesture.preview_sheet,
            was_held,
            rejected: gesture.rejected,
            source_current: self.pending_source_is_current(document) && !has_other_drafts,
            additive: gesture.additive,
            samples: gesture.samples().iter().copied().map(Into::into).collect(),
        })
    }

    /// Restore a detached, released gesture; only matching pen strokes can be retried.
    pub fn restore_recovered_ink(
        &mut self,
        document: &Document,
        recovered: &RecoveredInkGesture,
    ) -> Result<(), String> {
        if self.needs_input_tracking() {
            return Err("ink input is already active".into());
        }
        if recovered.samples.is_empty() || recovered.samples.len() > MAX_INPUT_SAMPLES {
            return Err("invalid recovered ink sample count".into());
        }
        if recovered.source_current && recovered.sheet != document.active_index() {
            return Err("recovered ink sheet does not match its workbook".into());
        }
        let settings: InkSettings = recovered.settings.into();
        if !settings.opacity.is_finite() || !(0.0..=1.0).contains(&settings.opacity) {
            return Err("invalid recovered ink opacity".into());
        }
        let samples = recovered.samples.iter().copied().map(InkSample::from);
        let buffer = match recovered.mode {
            InkMode::Pen | InkMode::Highlighter => {
                let tool = if recovered.mode == InkMode::Pen {
                    InkTool::Pen
                } else {
                    InkTool::Highlighter
                };
                let mut input = InkInput::new(tool, settings.color, settings.width)
                    .map_err(|error| error.to_string())?;
                for sample in samples {
                    input.push(sample).map_err(|error| error.to_string())?;
                }
                GestureBuffer::Draw(input)
            }
            InkMode::ObjectEraser | InkMode::PixelEraser | InkMode::Lasso => {
                if recovered.mode != InkMode::Lasso
                    && (!settings.eraser_radius.is_finite()
                        || !(0.0..=MAX_BOARD_COORDINATE).contains(&settings.eraser_radius))
                {
                    return Err("invalid recovered eraser radius".into());
                }
                let samples: Vec<_> = samples.collect();
                if samples
                    .iter()
                    .copied()
                    .any(|sample| !valid_path_sample(sample))
                {
                    return Err("invalid recovered gesture sample".into());
                }
                GestureBuffer::Path(samples)
            }
        };
        let recovery_block = if !recovered.source_current {
            Some(InkRecoveryBlock::StaleDocument)
        } else if !matches!(recovered.mode, InkMode::Pen | InkMode::Highlighter) {
            Some(InkRecoveryBlock::SceneDependent)
        } else {
            None
        };
        self.gesture = Some(Gesture {
            mode: recovered.mode,
            settings,
            owner: PointerOwner::Mouse,
            stamp: DocumentStamp::capture(document),
            transform: Xf {
                origin: Pos2::ZERO,
                zoom: 1.0,
            },
            canvas: Rect::from_min_size(Pos2::ZERO, egui::Vec2::ZERO),
            additive: recovered.additive,
            held: false,
            rejected: recovered.rejected,
            recovery_origin: Some((recovered.pointer, recovered.was_held)),
            recovery_block,
            preview_sheet: recovered.sheet,
            buffer,
        });
        self.recovery_revision = self.recovery_revision.wrapping_add(1);
        Ok(())
    }

    pub(crate) fn accepts_start_event(
        &self,
        event: &Event,
        frame_has_touch: bool,
        confirmed_pen: bool,
    ) -> bool {
        match event {
            Event::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                ..
            } => {
                (!frame_has_touch || confirmed_pen)
                    && self.touches.is_empty()
                    && !self.suppress_mouse
            }
            Event::Touch {
                device_id,
                id,
                phase: TouchPhase::Start,
                ..
            } => {
                !self.block_touch
                    && !self.owns_pointer()
                    && (self.touches.is_empty()
                        || self.touches.as_slice() == [TouchKey(*device_id, *id)])
            }
            _ => false,
        }
    }

    /// Find a new press after cancellation only once all earlier contacts have ended.
    pub(crate) fn restart_after_touch_cancel(&self, events: &[Event]) -> Option<usize> {
        let mut touches = self.touches.clone();
        let mut canceled = false;
        let mut restart = None;
        for (index, event) in events.iter().enumerate() {
            match event {
                Event::Touch {
                    device_id,
                    id,
                    phase,
                    ..
                } => {
                    let key = TouchKey(*device_id, *id);
                    match phase {
                        TouchPhase::Start => {
                            if !touches.is_empty() && !touches.contains(&key) {
                                restart = None;
                            }
                            if !touches.contains(&key) {
                                touches.push(key);
                            }
                        }
                        TouchPhase::End | TouchPhase::Cancel => {
                            touches.retain(|touch| *touch != key);
                            if *phase == TouchPhase::Cancel {
                                canceled = true;
                                restart = None;
                            }
                        }
                        TouchPhase::Move => {}
                    }
                }
                Event::PointerButton {
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                } if canceled && touches.is_empty() => restart = Some(index),
                Event::WindowFocused(false) => {
                    canceled = false;
                    restart = None;
                    touches.clear();
                }
                _ => {}
            }
        }
        restart
    }

    /// Explicitly discard the buffered gesture. Pointer ownership is suppressed
    /// until the current mouse button or all active touches are released.
    pub fn cancel(&mut self) -> bool {
        let had_gesture = self.gesture.take().is_some();
        if had_gesture {
            self.recovery_revision = self.recovery_revision.wrapping_add(1);
        }
        if self.mouse_down {
            self.suppress_mouse = true;
        }
        if !self.touches.is_empty() {
            self.block_touch = true;
            self.suppress_mouse = true;
        }
        had_gesture
    }

    /// Stop capturing an interrupted pointer without discarding its samples.
    /// The caller can retry the resulting released gesture or explicitly cancel it.
    pub fn interrupt(&mut self) -> bool {
        let Some(gesture) = self.gesture.as_mut().filter(|gesture| gesture.held) else {
            return false;
        };
        gesture.held = false;
        self.recovery_revision = self.recovery_revision.wrapping_add(1);
        if self.mouse_down {
            self.suppress_mouse = true;
        }
        if !self.touches.is_empty() {
            self.block_touch = true;
            self.suppress_mouse = true;
        }
        true
    }

    /// Whether a hidden canvas still needs pointer-release events.
    pub fn needs_input_tracking(&self) -> bool {
        self.gesture.is_some() || self.mouse_down || !self.touches.is_empty() || self.suppress_mouse
    }

    /// Retain the current frame's owned samples before suspending a hidden
    /// canvas, then track releases without accepting new gestures.
    pub fn suspend(
        &mut self,
        document: &Document,
        transform: Xf,
        events: &[Event],
    ) -> InkFrameOutput {
        self.suspend_with_pen(document, transform, events, &PenFrameData::default())
    }

    pub(crate) fn suspend_with_pen(
        &mut self,
        document: &Document,
        transform: Xf,
        events: &[Event],
        pen: &PenFrameData,
    ) -> InkFrameOutput {
        let mut output = InkFrameOutput::default();
        if let Some(gesture) = self.gesture.as_ref().filter(|gesture| gesture.held) {
            let input = InkFrameInput {
                events,
                canvas: gesture.canvas,
                transform,
                enabled: false,
                can_start: false,
                modifiers: egui::Modifiers::NONE,
            };
            if pen.issue.is_none() && pen.touch_issue.is_none() {
                self.sample_before_interruption(document, &input, pen, &mut output);
            }
            self.interrupt_with(InkCancelReason::Disabled, &mut output);
        }
        for (index, event) in events.iter().enumerate() {
            if pen.touch_pointer(index) {
                continue;
            }
            match event {
                Event::PointerButton {
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                }
                | Event::PointerGone => {
                    self.mouse_down = false;
                    self.mouse_owned = false;
                }
                Event::Touch {
                    device_id,
                    id,
                    phase: TouchPhase::Start,
                    ..
                } if !self.touches.is_empty() => {
                    let touch = TouchKey(*device_id, *id);
                    if !self.touches.contains(&touch) {
                        self.touches.push(touch);
                    }
                    self.block_touch = true;
                    self.suppress_mouse = true;
                }
                Event::Touch {
                    device_id,
                    id,
                    phase: TouchPhase::End | TouchPhase::Cancel,
                    ..
                } => {
                    let touch = TouchKey(*device_id, *id);
                    self.touches.retain(|active| *active != touch);
                }
                Event::WindowFocused(false) => {
                    self.touches.clear();
                    self.mouse_down = false;
                    self.mouse_owned = false;
                }
                _ => {}
            }
        }
        if self.touches.is_empty() {
            self.block_touch = false;
            self.pen_touch_lock = false;
            if !self.mouse_down {
                self.suppress_mouse = false;
            }
        }
        output.captured |= self.gesture.is_some() || self.pen_touch_lock;
        output
    }

    /// Retry a released gesture after a recoverable error. A rejected sample
    /// or changed document/camera must instead be canceled explicitly.
    pub fn retry(
        &mut self,
        document: &mut Document,
        scene: &Scene,
    ) -> Result<InkOutcome, InkControllerError> {
        let gesture = self.gesture.as_ref().ok_or(InkControllerError::NoGesture)?;
        gesture.ensure_retryable(document)?;
        let outcome = finish_gesture(gesture, document, scene)?;
        self.gesture = None;
        self.recovery_revision = self.recovery_revision.wrapping_add(1);
        Ok(outcome)
    }

    /// Retry a released pen/highlighter gesture with different finish options,
    /// retaining all accepted samples if the retry fails.
    pub fn retry_with_finish_options(
        &mut self,
        document: &mut Document,
        scene: &Scene,
        options: StrokeFinishOptions,
    ) -> Result<InkOutcome, InkControllerError> {
        let gesture = self.gesture.as_mut().ok_or(InkControllerError::NoGesture)?;
        gesture.ensure_retryable(document)?;
        if !matches!(&gesture.buffer, GestureBuffer::Draw(_)) {
            return Err(InkControllerError::NotDrawingGesture);
        }
        gesture.settings.finish = options;
        self.recovery_revision = self.recovery_revision.wrapping_add(1);
        self.retry(document, scene)
    }

    /// Process raw egui events. Touch force is optional; synthetic pointer
    /// events accompanying touches are ignored to avoid duplicate strokes.
    pub fn process_frame(
        &mut self,
        document: &mut Document,
        scene: &Scene,
        mode: Option<InkMode>,
        settings: InkSettings,
        input: InkFrameInput<'_>,
    ) -> InkFrameOutput {
        self.process_frame_with_pen(
            document,
            scene,
            mode,
            settings,
            input,
            &PenFrameData::default(),
        )
    }

    pub(crate) fn process_frame_with_pen(
        &mut self,
        document: &mut Document,
        scene: &Scene,
        mode: Option<InkMode>,
        settings: InkSettings,
        input: InkFrameInput<'_>,
        pen: &PenFrameData,
    ) -> InkFrameOutput {
        let mut output = InkFrameOutput::default();
        if (pen.issue.is_some() || pen.touch_issue.is_some())
            && self.gesture.as_ref().is_some_and(|gesture| gesture.held)
        {
            self.interrupt_with(InkCancelReason::PointerSourceChanged, &mut output);
        }
        let changed_tool = self.gesture.as_ref().is_some_and(|g| Some(g.mode) != mode);
        if self.gesture.as_ref().is_some_and(|gesture| gesture.held)
            && (!input.enabled || changed_tool)
        {
            self.sample_before_interruption(document, &input, pen, &mut output);
            self.interrupt_with(
                if changed_tool {
                    InkCancelReason::ToolChanged
                } else {
                    InkCancelReason::Disabled
                },
                &mut output,
            );
        }

        let frame_has_touch = input
            .events
            .iter()
            .any(|event| matches!(event, Event::Touch { .. }));
        let mut focus_lost = false;
        let mut reported_input_issue = false;
        for (index, event) in input.events.iter().enumerate() {
            let pen_sample = pen.sample(index);
            let owner = pointer_owner(pen_sample);
            match event {
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                } => {
                    if pen.touch_pointer(index) {
                        if *pressed && self.owns_pointer() && !self.owns_pen() {
                            self.interrupt_with(InkCancelReason::PointerSourceChanged, &mut output);
                        }
                        if *pressed
                            && let Some(issue) = pen.touch_issue
                            && mode.is_some()
                            && input.enabled
                            && !focus_lost
                            && input.can_start
                            && can_start_at(input.canvas, *pos)
                        {
                            output
                                .errors
                                .push(InkControllerError::UncorrelatedTouchInput {
                                    issue,
                                    position: *pos,
                                });
                            reported_input_issue = true;
                        }
                        output.captured |= self.pen_touch_lock
                            || (mode.is_some() && input.enabled && input.can_start);
                        continue;
                    }
                    if !frame_has_touch
                        && self.touches.is_empty()
                        && self.conflicting_pointer(owner)
                    {
                        self.interrupt_with(InkCancelReason::PointerSourceChanged, &mut output);
                        continue;
                    }
                    let owned_pen = matches!(owner, PointerOwner::Pen(_)) && self.owns(owner);
                    let starting_pen = *pressed
                        && matches!(owner, PointerOwner::Pen(_))
                        && self.gesture.is_none()
                        && self.touches.is_empty()
                        && !self.suppress_mouse;
                    let suppressing_owned_mouse =
                        self.suppress_mouse && self.mouse_down && self.mouse_owned;
                    let ignore_mouse =
                        (frame_has_touch || !self.touches.is_empty() || self.suppress_mouse)
                            && !owned_pen
                            && !starting_pen;
                    if (!frame_has_touch && self.touches.is_empty()) || owned_pen || starting_pen {
                        self.mouse_down = *pressed;
                    } else if !pressed {
                        self.mouse_down = false;
                    }
                    if ignore_mouse {
                        output.captured |= suppressing_owned_mouse;
                        if !pressed {
                            self.mouse_owned = false;
                        }
                        if !pressed && !frame_has_touch && self.touches.is_empty() {
                            self.suppress_mouse = false;
                        }
                        continue;
                    }
                    if *pressed {
                        if let Some(issue) = pen.issue
                            && mode.is_some()
                            && input.enabled
                            && !focus_lost
                            && input.can_start
                            && can_start_at(input.canvas, *pos)
                        {
                            self.mouse_owned = true;
                            self.suppress_mouse = true;
                            output.captured = true;
                            output
                                .errors
                                .push(InkControllerError::UncorrelatedPenInput {
                                    issue,
                                    position: *pos,
                                });
                            reported_input_issue = true;
                            continue;
                        }
                        if let Some(mode) = mode.filter(|_| {
                            input.enabled
                                && !focus_lost
                                && input.can_start
                                && can_start_at(input.canvas, *pos)
                        }) {
                            output.captured = true;
                            self.mouse_owned = true;
                            if let Err(error) = self.start(
                                document,
                                mode,
                                settings,
                                PointerSample {
                                    owner,
                                    position: *pos,
                                    pressure: pen_sample.map(|sample| sample.pressure),
                                },
                                *modifiers,
                                &input,
                            ) {
                                self.suppress_mouse = true;
                                output.errors.push(error);
                            }
                        }
                    } else if self.owns(owner) {
                        output.captured = true;
                        self.release(
                            document,
                            scene,
                            PointerSample {
                                owner,
                                position: *pos,
                                pressure: pen_sample.map(|sample| sample.pressure),
                            },
                            &input,
                            &mut output,
                        );
                    }
                    if !pressed {
                        self.mouse_owned = false;
                    }
                }
                Event::PointerMoved(pos)
                    if !pen.touch_pointer(index)
                        && ((!frame_has_touch && self.touches.is_empty())
                            || (matches!(owner, PointerOwner::Pen(_)) && self.owns(owner))) =>
                {
                    if self.conflicting_pointer(owner) {
                        self.interrupt_with(InkCancelReason::PointerSourceChanged, &mut output);
                    } else if self.suppress_mouse && self.mouse_down && self.mouse_owned {
                        output.captured = true;
                    } else if !self.suppress_mouse && self.owns(owner) {
                        output.captured = true;
                        self.sample(
                            document,
                            PointerSample {
                                owner,
                                position: *pos,
                                pressure: pen_sample.map(|sample| sample.pressure),
                            },
                            &input,
                            &mut output,
                        );
                    }
                }
                Event::Touch {
                    device_id,
                    id,
                    phase,
                    pos,
                    force,
                } => {
                    let key = TouchKey(*device_id, *id);
                    output.captured |= self.pen_touch_lock;
                    match phase {
                        TouchPhase::Start => {
                            if !self.touches.contains(&key) {
                                self.touches.push(key);
                            }
                            let input_issue = pen
                                .touch_issue
                                .map(|issue| InkControllerError::UncorrelatedTouchInput {
                                    issue,
                                    position: *pos,
                                })
                                .or_else(|| {
                                    pen.issue.map(|issue| {
                                        InkControllerError::UncorrelatedPenInput {
                                            issue,
                                            position: *pos,
                                        }
                                    })
                                });
                            if let Some(error) = input_issue {
                                self.suppress_mouse = true;
                                self.block_touch = true;
                                if mode.is_some()
                                    && input.enabled
                                    && !focus_lost
                                    && input.can_start
                                    && can_start_at(input.canvas, *pos)
                                {
                                    output.captured = true;
                                    if !reported_input_issue {
                                        output.errors.push(error);
                                        reported_input_issue = true;
                                    }
                                }
                            } else if self.owns_pen() {
                                self.pen_touch_lock = true;
                                self.block_touch = true;
                                output.captured = true;
                            } else if self.touches.len() > 1 || self.owns_pointer() {
                                self.suppress_mouse = true;
                                self.block_touch = true;
                                self.interrupt_with(InkCancelReason::Multitouch, &mut output);
                            } else if !self.block_touch
                                && let Some(mode) = mode.filter(|_| {
                                    input.enabled
                                        && !focus_lost
                                        && input.can_start
                                        && can_start_at(input.canvas, *pos)
                                })
                            {
                                self.suppress_mouse = true;
                                output.captured = true;
                                if let Err(error) = self.start(
                                    document,
                                    mode,
                                    settings,
                                    PointerSample {
                                        owner: PointerOwner::Touch(key),
                                        position: *pos,
                                        pressure: *force,
                                    },
                                    input.modifiers,
                                    &input,
                                ) {
                                    self.block_touch = true;
                                    output.errors.push(error);
                                }
                            } else {
                                self.suppress_mouse = true;
                            }
                        }
                        TouchPhase::Move
                            if !self.block_touch && self.owns(PointerOwner::Touch(key)) =>
                        {
                            output.captured = true;
                            self.sample(
                                document,
                                PointerSample {
                                    owner: PointerOwner::Touch(key),
                                    position: *pos,
                                    pressure: *force,
                                },
                                &input,
                                &mut output,
                            );
                        }
                        TouchPhase::End => {
                            if !self.block_touch && self.owns(PointerOwner::Touch(key)) {
                                output.captured = true;
                                self.release(
                                    document,
                                    scene,
                                    PointerSample {
                                        owner: PointerOwner::Touch(key),
                                        position: *pos,
                                        pressure: *force,
                                    },
                                    &input,
                                    &mut output,
                                );
                            }
                            self.touches.retain(|touch| *touch != key);
                            if self.touches.is_empty() {
                                self.block_touch = false;
                                if self.pen_touch_lock && !self.mouse_down {
                                    self.suppress_mouse = false;
                                }
                                self.pen_touch_lock = false;
                            }
                        }
                        TouchPhase::Cancel => {
                            if self.owns(PointerOwner::Touch(key)) {
                                self.sample(
                                    document,
                                    PointerSample {
                                        owner: PointerOwner::Touch(key),
                                        position: *pos,
                                        pressure: *force,
                                    },
                                    &input,
                                    &mut output,
                                );
                                self.interrupt_with(InkCancelReason::TouchCanceled, &mut output);
                            }
                            self.touches.retain(|touch| *touch != key);
                            self.block_touch = !self.touches.is_empty();
                            if self.touches.is_empty() {
                                if self.pen_touch_lock && !self.mouse_down {
                                    self.suppress_mouse = false;
                                }
                                self.pen_touch_lock = false;
                            }
                        }
                        _ => {}
                    }
                }
                Event::Key {
                    key: Key::Escape,
                    pressed: true,
                    repeat: false,
                    ..
                } => {
                    self.cancel_with(InkCancelReason::Escape, &mut output);
                }
                Event::WindowFocused(false) => {
                    if self.gesture.as_ref().is_some_and(|gesture| gesture.held) {
                        self.interrupt_with(InkCancelReason::FocusLost, &mut output);
                    }
                    self.touches.clear();
                    self.block_touch = false;
                    self.pen_touch_lock = false;
                    self.suppress_mouse = false;
                    self.mouse_down = false;
                    self.mouse_owned = false;
                    focus_lost = true;
                }
                Event::PointerGone => {
                    if pen.touch_issue.is_none()
                        && pen.touch_pointers.get(index).copied().unwrap_or(false)
                    {
                        continue;
                    }
                    if self.owns_pointer()
                        || (self.suppress_mouse && self.mouse_down && self.mouse_owned)
                    {
                        output.captured = true;
                        self.interrupt_with(InkCancelReason::PointerGone, &mut output);
                    }
                    self.mouse_down = false;
                    self.mouse_owned = false;
                    if self.touches.is_empty() {
                        self.suppress_mouse = false;
                    }
                }
                _ => {}
            }
        }
        if let Some(id) = pen.canceled
            && self
                .gesture
                .as_ref()
                .is_some_and(|gesture| gesture.owner == PointerOwner::Pen(id))
        {
            self.interrupt_with(InkCancelReason::PointerGone, &mut output);
            self.mouse_down = false;
            self.mouse_owned = false;
            if self.touches.is_empty() {
                self.suppress_mouse = false;
            }
        }
        if self.touches.is_empty() && !self.mouse_down {
            self.suppress_mouse = false;
        }
        output.captured |= self.gesture.is_some() || self.pen_touch_lock;
        output
    }

    fn cancel_with(&mut self, reason: InkCancelReason, output: &mut InkFrameOutput) {
        if self.cancel() {
            output.captured = true;
            output.outcomes.push(InkOutcome::Canceled(reason));
        }
    }

    fn interrupt_with(&mut self, reason: InkCancelReason, output: &mut InkFrameOutput) {
        if self.interrupt() {
            output.captured = true;
            output.errors.push(InkControllerError::Interrupted(reason));
        }
    }

    fn sample_before_interruption(
        &mut self,
        document: &Document,
        input: &InkFrameInput<'_>,
        pen: &PenFrameData,
        output: &mut InkFrameOutput,
    ) {
        let frame_has_touch = input
            .events
            .iter()
            .any(|event| matches!(event, Event::Touch { .. }));
        for (index, event) in input.events.iter().enumerate() {
            if pen.touch_pointer(index) {
                continue;
            }
            let pen_sample = pen.sample(index);
            let owner = pointer_owner(pen_sample);
            let (pointer, released) = match event {
                Event::PointerMoved(position)
                    if self.owns(owner)
                        && (!frame_has_touch || matches!(owner, PointerOwner::Pen(_))) =>
                {
                    (
                        PointerSample {
                            owner,
                            position: *position,
                            pressure: pen_sample.map(|sample| sample.pressure),
                        },
                        false,
                    )
                }
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                } if self.owns(owner)
                    && (!frame_has_touch || matches!(owner, PointerOwner::Pen(_))) =>
                {
                    (
                        PointerSample {
                            owner,
                            position: *pos,
                            pressure: pen_sample.map(|sample| sample.pressure),
                        },
                        true,
                    )
                }
                Event::Touch {
                    device_id,
                    id,
                    phase: phase @ (TouchPhase::Move | TouchPhase::End | TouchPhase::Cancel),
                    pos,
                    force,
                } if self.owns(PointerOwner::Touch(TouchKey(*device_id, *id))) => (
                    PointerSample {
                        owner: PointerOwner::Touch(TouchKey(*device_id, *id)),
                        position: *pos,
                        pressure: *force,
                    },
                    *phase != TouchPhase::Move,
                ),
                Event::WindowFocused(false) => break,
                Event::PointerGone if self.owns_pointer() => break,
                Event::Touch {
                    device_id,
                    id,
                    phase: TouchPhase::Start,
                    ..
                } if !self.owns_pen()
                    && !self.owns(PointerOwner::Touch(TouchKey(*device_id, *id))) =>
                {
                    break;
                }
                _ => continue,
            };
            self.sample(document, pointer, input, output);
            if released {
                break;
            }
        }
    }

    fn owns(&self, owner: PointerOwner) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|gesture| gesture.owner == owner && gesture.held)
    }

    fn owns_pointer(&self) -> bool {
        self.gesture.as_ref().is_some_and(|gesture| {
            gesture.held && matches!(gesture.owner, PointerOwner::Mouse | PointerOwner::Pen(_))
        })
    }

    fn owns_pen(&self) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|gesture| gesture.held && matches!(gesture.owner, PointerOwner::Pen(_)))
    }

    pub(crate) fn blocks_touch_navigation(&self) -> bool {
        self.owns_pen() || self.pen_touch_lock
    }

    fn conflicting_pointer(&self, owner: PointerOwner) -> bool {
        self.owns_pointer() && !self.owns(owner)
    }

    fn start(
        &mut self,
        document: &Document,
        mode: InkMode,
        settings: InkSettings,
        pointer: PointerSample,
        modifiers: egui::Modifiers,
        input: &InkFrameInput<'_>,
    ) -> Result<(), InkControllerError> {
        if self.gesture.is_some() {
            return Err(InkControllerError::StartWhilePending {
                position: pointer.position,
                pressure: pointer.pressure,
            });
        }
        let sample = sheet_sample(input.transform, pointer.position, pointer.pressure)?;
        if !settings.opacity.is_finite() || !(0.0..=1.0).contains(&settings.opacity) {
            return Err(InkControllerError::InvalidSettings {
                reason: "opacity must be between 0 and 1".into(),
                sample,
            });
        }
        let buffer = match mode {
            InkMode::Pen | InkMode::Highlighter => {
                let tool = if mode == InkMode::Pen {
                    InkTool::Pen
                } else {
                    InkTool::Highlighter
                };
                let mut drawing =
                    InkInput::new(tool, settings.color, settings.width).map_err(|error| {
                        InkControllerError::InvalidSettings {
                            reason: error.to_string(),
                            sample,
                        }
                    })?;
                drawing
                    .push(sample)
                    .map_err(InkControllerError::InvalidSample)?;
                GestureBuffer::Draw(drawing)
            }
            InkMode::ObjectEraser | InkMode::PixelEraser | InkMode::Lasso => {
                if mode != InkMode::Lasso
                    && (!settings.eraser_radius.is_finite()
                        || !(0.0..=MAX_BOARD_COORDINATE).contains(&settings.eraser_radius))
                {
                    return Err(InkControllerError::InvalidSettings {
                        reason: "invalid eraser radius".into(),
                        sample,
                    });
                }
                if !valid_path_sample(sample) {
                    return Err(InkControllerError::InvalidPathSample(sample));
                }
                GestureBuffer::Path(vec![sample])
            }
        };
        self.gesture = Some(Gesture {
            mode,
            settings,
            owner: pointer.owner,
            stamp: DocumentStamp::capture(document),
            transform: input.transform,
            canvas: input.canvas,
            additive: modifiers.shift || modifiers.command || modifiers.ctrl,
            held: true,
            rejected: false,
            recovery_origin: None,
            recovery_block: None,
            preview_sheet: document.active_index(),
            buffer,
        });
        self.recovery_revision = self.recovery_revision.wrapping_add(1);
        Ok(())
    }

    fn sample(
        &mut self,
        document: &Document,
        pointer: PointerSample,
        input: &InkFrameInput<'_>,
        output: &mut InkFrameOutput,
    ) {
        if !self.owns(pointer.owner) {
            return;
        }
        let sample = match sheet_sample(input.transform, pointer.position, pointer.pressure) {
            Ok(sample) => sample,
            Err(error) => {
                self.gesture.as_mut().expect("owned").rejected = true;
                self.recovery_revision = self.recovery_revision.wrapping_add(1);
                output.errors.push(error);
                return;
            }
        };
        let gesture = self.gesture.as_mut().expect("owned");
        let mut changed = true;
        if !gesture.stamp.matches(document) {
            gesture.rejected = true;
            output.errors.push(InkControllerError::StaleDocument {
                sample: Some(sample),
            });
        } else if gesture.transform != input.transform || gesture.canvas != input.canvas {
            gesture.rejected = true;
            output.errors.push(InkControllerError::CameraChanged {
                sample: Some(sample),
            });
        } else if gesture.samples().last() != Some(&sample) {
            if let Err(error) = gesture.push(sample) {
                output.errors.push(error);
            }
        } else {
            changed = false;
        }
        if changed {
            self.recovery_revision = self.recovery_revision.wrapping_add(1);
        }
    }

    fn release(
        &mut self,
        document: &mut Document,
        scene: &Scene,
        pointer: PointerSample,
        input: &InkFrameInput<'_>,
        output: &mut InkFrameOutput,
    ) {
        self.sample(document, pointer, input, output);
        if let Some(gesture) = &mut self.gesture {
            gesture.held = false;
            self.recovery_revision = self.recovery_revision.wrapping_add(1);
        }
        match self.retry(document, scene) {
            Ok(outcome) => output.outcomes.push(outcome),
            Err(error) => output.errors.push(error),
        }
    }

    /// Draw the accepted raw samples without modifying the document.
    pub fn paint_preview(&self, painter: &Painter, transform: Xf, accent: Color32) {
        let Some(gesture) = &self.gesture else { return };
        if !valid_transform(transform) {
            return;
        }
        let points: Vec<_> = gesture
            .samples()
            .iter()
            .map(|sample| transform.pos(sample.position))
            .collect();
        let Some(first) = points.first().copied() else {
            return;
        };
        let (width, color) = match gesture.mode {
            InkMode::Pen | InkMode::Highlighter => {
                let opacity = gesture.settings.opacity
                    * if gesture.mode == InkMode::Highlighter {
                        0.28
                    } else {
                        1.0
                    };
                (
                    gesture.settings.width * transform.zoom,
                    color32(gesture.settings.color).gamma_multiply(opacity),
                )
            }
            InkMode::ObjectEraser | InkMode::PixelEraser => (
                gesture.settings.eraser_radius * 2.0 * transform.zoom,
                accent.gamma_multiply(0.35),
            ),
            InkMode::Lasso => (1.5, accent),
        };
        let width = width.max(0.5);
        if points.iter().all(|point| *point == first) {
            painter.circle_filled(first, (width / 2.0).max(0.5), color);
        } else if gesture.mode == InkMode::Lasso && points.len() >= 3 {
            painter.add(Shape::closed_line(points, Stroke::new(width, color)));
        } else {
            painter.add(Shape::line(points, Stroke::new(width, color)));
        }
    }
}

fn valid_transform(transform: Xf) -> bool {
    transform.origin.x.is_finite()
        && transform.origin.y.is_finite()
        && transform.zoom.is_finite()
        && transform.zoom > 0.0
}

fn can_start_at(canvas: Rect, position: Pos2) -> bool {
    !position.x.is_finite() || !position.y.is_finite() || canvas.contains(position)
}

fn sheet_sample(
    transform: Xf,
    position: Pos2,
    pressure: Option<f32>,
) -> Result<InkSample, InkControllerError> {
    if !valid_transform(transform) {
        return Err(InkControllerError::InvalidTransform { position, pressure });
    }
    Ok(InkSample::new(transform.sheet(position), pressure))
}

fn valid_position(point: Vec2) -> bool {
    point.x.is_finite()
        && point.y.is_finite()
        && point.x.abs() <= MAX_BOARD_COORDINATE
        && point.y.abs() <= MAX_BOARD_COORDINATE
}

fn valid_path_sample(sample: InkSample) -> bool {
    valid_position(sample.position)
        && sample
            .pressure
            .is_none_or(|pressure| pressure.is_finite() && (0.0..=1.0).contains(&pressure))
}

fn finish_gesture(
    gesture: &Gesture,
    document: &mut Document,
    scene: &Scene,
) -> Result<InkOutcome, InkControllerError> {
    match &gesture.buffer {
        GestureBuffer::Draw(input) => {
            let stroke = input
                .finish(gesture.settings.finish)
                .map_err(InkControllerError::Ink)?;
            let mut object = CanvasObject::ink(stroke);
            object.band = gesture.settings.band;
            object.style.opacity = gesture.settings.opacity;
            document
                .add_canvas_object(object)
                .map(InkOutcome::Drawn)
                .map_err(InkControllerError::Edit)
        }
        GestureBuffer::Path(samples) => {
            let path: Vec<_> = samples.iter().map(|sample| sample.position).collect();
            match gesture.mode {
                InkMode::ObjectEraser => {
                    erase_objects(document, scene, &path, gesture.settings.eraser_radius)
                }
                InkMode::PixelEraser => {
                    erase_pixels(document, scene, &path, gesture.settings.eraser_radius)
                }
                InkMode::Lasso => select_lasso(document.sheet(), scene, &path, gesture.additive),
                InkMode::Pen | InkMode::Highlighter => unreachable!("drawing uses an ink buffer"),
            }
        }
    }
}

fn validate_ink_scene<'a>(
    sheet: &Sheet,
    scene: &'a Scene,
) -> Result<HashMap<CanvasObjectId, &'a thinoda_core::SceneItem>, InkControllerError> {
    let objects: HashMap<_, _> = sheet
        .canvas
        .objects
        .iter()
        .map(|object| (object.id, object))
        .collect();
    let mut ink_items = HashMap::new();
    for item in &scene.items {
        let SceneId::Object(id) = item.id else {
            continue;
        };
        let Some(object) = objects.get(&id) else {
            return Err(InkControllerError::StaleScene(id));
        };
        match (&object.kind, &item.geometry) {
            (
                CanvasObjectKind::Ink(stroke),
                SceneGeometry::Ink {
                    points,
                    width,
                    center,
                    rotation,
                },
            ) if stroke.points == *points
                && stroke.width == *width
                && object.rect.center() == *center
                && object.rotation == *rotation
                && object_clip_rect(sheet, object) == item.clip =>
            {
                if ink_items.insert(id, item).is_some() {
                    return Err(InkControllerError::StaleScene(id));
                }
            }
            (CanvasObjectKind::Ink(_), _) | (_, SceneGeometry::Ink { .. }) => {
                return Err(InkControllerError::StaleScene(id));
            }
            _ => {}
        }
    }
    for object in &sheet.canvas.objects {
        if matches!(object.kind, CanvasObjectKind::Ink(_)) && !ink_items.contains_key(&object.id) {
            return Err(InkControllerError::StaleScene(object.id));
        }
    }
    Ok(ink_items)
}

fn require_unlocked(sheet: &Sheet, ids: &[CanvasObjectId]) -> Result<(), InkControllerError> {
    if ids.is_empty() {
        return Ok(());
    }
    let members: Vec<_> = ids.iter().copied().map(BoardMember::Object).collect();
    let expanded = sheet
        .expand_board_selection(&members)
        .map_err(|error| InkControllerError::Edit(EditError::Invalid(error)))?;
    let locked: Vec<_> = expanded
        .into_iter()
        .filter(|member| sheet.is_board_member_locked(*member))
        .collect();
    if locked.is_empty() {
        Ok(())
    } else {
        Err(InkControllerError::Locked(locked))
    }
}

fn erase_objects(
    document: &mut Document,
    scene: &Scene,
    path: &[Vec2],
    radius: f32,
) -> Result<InkOutcome, InkControllerError> {
    validate_ink_scene(document.sheet(), scene)?;
    let ids = eraser_hits(scene, path, radius).map_err(InkControllerError::Geometry)?;
    if ids.is_empty() {
        return Ok(InkOutcome::Unchanged);
    }
    require_unlocked(document.sheet(), &ids)?;
    document
        .edit_sheet("Erase ink", |sheet| {
            sheet
                .remove_canvas_objects(&ids)
                .map_err(EditError::Invalid)
        })
        .map_err(InkControllerError::Edit)?;
    Ok(InkOutcome::ObjectErased(ids))
}

fn erase_pixels(
    document: &mut Document,
    scene: &Scene,
    path: &[Vec2],
    radius: f32,
) -> Result<InkOutcome, InkControllerError> {
    let sheet = document.sheet();
    let items = validate_ink_scene(sheet, scene)?;
    let hits = eraser_hits(scene, path, radius).map_err(InkControllerError::Geometry)?;
    require_unlocked(sheet, &hits)?;
    let objects: HashMap<_, _> = sheet
        .canvas
        .objects
        .iter()
        .map(|object| (object.id, object))
        .collect();
    let path_segments = path.len().saturating_sub(1).max(1);
    let mut checks = 0usize;
    let mut plans: HashMap<CanvasObjectId, Vec<InkStroke>> = HashMap::new();
    let mut total_points = sheet.canvas.ink_point_count();
    let mut total_objects = sheet.canvas.objects.len();
    let mut changed = Vec::new();
    for id in hits {
        let object = objects.get(&id).ok_or(InkControllerError::StaleScene(id))?;
        let CanvasObjectKind::Ink(stroke) = &object.kind else {
            return Err(InkControllerError::StaleScene(id));
        };
        checks = checks.saturating_add(
            stroke
                .points
                .len()
                .saturating_sub(1)
                .max(1)
                .saturating_mul(path_segments),
        );
        if checks > MAX_GEOMETRY_CHECKS {
            return Err(InkControllerError::Geometry(GeometryError::TooComplex {
                limit: MAX_GEOMETRY_CHECKS,
            }));
        }
        let clip = items[&id].clip;
        let result = pixel_erase_stroke(
            stroke,
            path,
            PixelEraseOptions {
                center: object.rect.center(),
                rotation: object.rotation,
                radius,
                clip,
            },
        )
        .map_err(|error| InkControllerError::Pixel { object: id, error })?;
        let fragments = match result {
            PixelEraseResult::Unchanged => continue,
            PixelEraseResult::Erased => Vec::new(),
            PixelEraseResult::Fragments(fragments) => fragments,
        };
        for fragment in &fragments {
            let mut replacement = (*object).clone();
            replacement.rect =
                points_bounds(&fragment.points).expect("validated fragment is nonempty");
            replacement.rotation = 0.0;
            replacement.kind = CanvasObjectKind::Ink(fragment.clone());
            if object_clip_rect(sheet, &replacement) != clip {
                return Err(InkControllerError::ClipWouldChange(id));
            }
        }
        total_objects = total_objects - 1 + fragments.len();
        total_points = total_points - stroke.points.len()
            + fragments
                .iter()
                .map(|fragment| fragment.points.len())
                .sum::<usize>();
        changed.push(id);
        plans.insert(id, fragments);
    }
    if changed.is_empty() {
        return Ok(InkOutcome::Unchanged);
    }
    if total_objects > MAX_CANVAS_OBJECTS {
        return Err(InkControllerError::Limit {
            what: "canvas object count",
            limit: MAX_CANVAS_OBJECTS,
        });
    }
    if total_points > MAX_TOTAL_INK_POINTS {
        return Err(InkControllerError::Limit {
            what: "ink point count",
            limit: MAX_TOTAL_INK_POINTS,
        });
    }
    let (originals, fragments, replacements) = document
        .edit_sheet("Pixel erase", |sheet| {
            let old = std::mem::take(&mut sheet.canvas.objects);
            let mut objects = Vec::with_capacity(total_objects);
            let mut originals = Vec::new();
            let mut fragment_ids = Vec::new();
            let mut mappings = Vec::new();
            let mut erased = Vec::new();
            for object in old {
                let Some(replacements) = plans.remove(&object.id) else {
                    objects.push(object);
                    continue;
                };
                originals.push(object.id);
                if replacements.is_empty() {
                    erased.push(object.id);
                    mappings.push(InkReplacement {
                        original: object.id,
                        fragments: Vec::new(),
                    });
                    objects.push(object);
                    continue;
                }
                let mut mapped_ids = Vec::with_capacity(replacements.len());
                for (index, stroke) in replacements.into_iter().enumerate() {
                    let mut replacement = object.clone();
                    if index > 0 {
                        replacement.id = sheet.alloc_canvas_object_id();
                    }
                    replacement.rect =
                        points_bounds(&stroke.points).expect("validated fragment is nonempty");
                    replacement.rotation = 0.0;
                    replacement.kind = CanvasObjectKind::Ink(stroke);
                    fragment_ids.push(replacement.id);
                    mapped_ids.push(replacement.id);
                    objects.push(replacement);
                }
                mappings.push(InkReplacement {
                    original: object.id,
                    fragments: mapped_ids,
                });
            }
            sheet.canvas.objects = objects;
            if !erased.is_empty() {
                sheet
                    .remove_canvas_objects(&erased)
                    .map_err(EditError::Invalid)?;
            }
            Ok((originals, fragment_ids, mappings))
        })
        .map_err(InkControllerError::Edit)?;
    Ok(InkOutcome::PixelErased {
        originals,
        fragments,
        replacements,
    })
}

fn select_lasso(
    sheet: &Sheet,
    scene: &Scene,
    path: &[Vec2],
    additive: bool,
) -> Result<InkOutcome, InkControllerError> {
    let mut hits = lasso_hits(scene, path).map_err(InkControllerError::Geometry)?;
    let mut members = Vec::new();
    for hit in &hits {
        match *hit {
            SceneHit::Object(id) => {
                if sheet.canvas.object(id).is_none() {
                    return Err(InkControllerError::StaleSelection(*hit));
                }
                members.push(BoardMember::Object(id));
            }
            SceneHit::Topic(id) => {
                if sheet.topic(id).is_none() || sheet.is_anchor(id) {
                    return Err(InkControllerError::StaleSelection(*hit));
                }
                if let Some(member) = sheet.board_member_for_topic(id) {
                    members.push(member);
                }
            }
        }
    }
    if !members.is_empty() {
        let expanded = sheet
            .expand_board_selection(&members)
            .map_err(|error| InkControllerError::Edit(EditError::Invalid(error)))?;
        let mut seen: HashSet<_> = hits.iter().copied().collect();
        for member in expanded {
            let hit = match member {
                BoardMember::Object(id) => SceneHit::Object(id),
                BoardMember::Card(id) => SceneHit::Topic(id),
            };
            if seen.insert(hit) {
                hits.push(hit);
            }
        }
    }
    Ok(InkOutcome::Selected(InkSelection { hits, additive }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Modifiers, pos2, vec2};
    use thinoda_core::{
        ApproxMeasure, CanvasConnector, Card, CardKind, Endpoint, Rect as BoardRect, ShapeKind,
        Workbook, build_scene, layout_sheet,
    };

    #[test]
    fn document_stamp_rejects_a_reused_sheet_address() {
        let original = Document::default();
        let replacement = original.clone();
        let mut stamp = DocumentStamp::capture(&original);
        stamp.identity = std::ptr::from_ref(replacement.sheet()) as usize;
        assert_eq!(stamp.state, replacement.state_id());
        assert_eq!(stamp.sheet, replacement.active_index());
        assert!(!stamp.matches(&replacement));
    }

    fn scene(document: &Document) -> Scene {
        build_scene(
            document.sheet(),
            &layout_sheet(document.sheet(), &ApproxMeasure, false),
        )
    }

    fn xf() -> Xf {
        Xf {
            origin: Pos2::ZERO,
            zoom: 1.0,
        }
    }

    fn frame(
        controller: &mut InkController,
        document: &mut Document,
        scene: &Scene,
        mode: Option<InkMode>,
        settings: InkSettings,
        transform: Xf,
        events: &[Event],
    ) -> InkFrameOutput {
        controller.process_frame(
            document,
            scene,
            mode,
            settings,
            InkFrameInput {
                events,
                canvas: Rect::from_min_max(pos2(-2_000.0, -2_000.0), pos2(2_000.0, 2_000.0)),
                transform,
                enabled: true,
                can_start: true,
                modifiers: Modifiers::NONE,
            },
        )
    }

    fn pen_frame(
        controller: &mut InkController,
        document: &mut Document,
        scene: &Scene,
        events: &[Event],
        samples: Vec<Option<PenSample>>,
        canceled: Option<i32>,
    ) -> InkFrameOutput {
        controller.process_frame_with_pen(
            document,
            scene,
            Some(InkMode::Pen),
            InkSettings {
                finish: StrokeFinishOptions {
                    tolerance: 0.0,
                    smooth: false,
                },
                ..InkSettings::default()
            },
            InkFrameInput {
                events,
                canvas: Rect::from_min_max(pos2(-2_000.0, -2_000.0), pos2(2_000.0, 2_000.0)),
                transform: xf(),
                enabled: true,
                can_start: true,
                modifiers: Modifiers::NONE,
            },
            &PenFrameData {
                samples,
                canceled,
                ..PenFrameData::default()
            },
        )
    }

    fn touch_pointer_frame(
        controller: &mut InkController,
        document: &mut Document,
        scene: &Scene,
        events: &[Event],
        pointers: Vec<bool>,
        issue: Option<TouchPointerIssue>,
    ) -> InkFrameOutput {
        controller.process_frame_with_pen(
            document,
            scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            InkFrameInput {
                events,
                canvas: Rect::from_min_max(pos2(-2_000.0, -2_000.0), pos2(2_000.0, 2_000.0)),
                transform: xf(),
                enabled: true,
                can_start: true,
                modifiers: Modifiers::NONE,
            },
            &PenFrameData {
                touch_pointers: pointers,
                touch_issue: issue,
                ..PenFrameData::default()
            },
        )
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    fn touch(id: u64, phase: TouchPhase, pos: Pos2, force: Option<f32>) -> Event {
        Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(id),
            phase,
            pos,
            force,
        }
    }

    fn mouse_gesture(
        controller: &mut InkController,
        document: &mut Document,
        scene: &Scene,
        mode: InkMode,
        settings: InkSettings,
        points: &[Pos2],
    ) -> InkFrameOutput {
        let mut events = vec![button(points[0], true)];
        events.extend(points[1..].iter().copied().map(Event::PointerMoved));
        events.push(button(*points.last().unwrap(), false));
        frame(
            controller,
            document,
            scene,
            Some(mode),
            settings,
            xf(),
            &events,
        )
    }

    fn stroke(points: &[(f32, f32)]) -> CanvasObject {
        CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::BLACK,
            2.0,
            points.iter().map(|&(x, y)| Vec2::new(x, y)).collect(),
        ))
    }

    #[test]
    fn mouse_preview_is_transient_and_release_commits_one_undoable_stroke() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings {
            color: Color::rgb(10, 20, 30),
            width: 4.0,
            opacity: 0.6,
            band: Band::Below,
            finish: StrokeFinishOptions {
                tolerance: 0.0,
                smooth: false,
            },
            ..InkSettings::default()
        };
        let transform = Xf {
            origin: pos2(100.0, 50.0),
            zoom: 2.0,
        };
        let down = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            transform,
            &[button(pos2(100.0, 50.0), true)],
        );
        assert!(down.captured && down.errors.is_empty());
        let moved = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            transform,
            &[Event::PointerMoved(pos2(120.0, 50.0))],
        );
        assert!(moved.captured && moved.errors.is_empty());
        assert_eq!(controller.pending_samples().len(), 2);
        assert!(document.sheet().canvas.objects.is_empty());
        assert!(!document.can_undo());

        let ctx = egui::Context::default();
        let mut output = ctx.run_pass(egui::RawInput::default(), |ctx| {
            controller.paint_preview(
                &ctx.layer_painter(egui::LayerId::background()),
                transform,
                Color32::BLUE,
            );
        });
        assert!(!output.shapes.is_empty());
        output.textures_delta.clear();
        assert!(document.sheet().canvas.objects.is_empty());

        let up = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            transform,
            &[button(pos2(130.0, 50.0), false)],
        );
        assert!(up.errors.is_empty());
        assert!(matches!(up.outcomes.as_slice(), [InkOutcome::Drawn(_)]));
        assert!(!controller.has_pending_gesture());
        assert_eq!(document.undo_label(), Some("Draw"));
        let object = &document.sheet().canvas.objects[0];
        assert_eq!(object.band, Band::Below);
        assert_eq!(object.style.opacity, 0.6);
        let CanvasObjectKind::Ink(ink) = &object.kind else {
            panic!("expected ink")
        };
        assert_eq!(
            &*ink.points,
            &[Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(15.0, 0.0)]
        );
        assert!(ink.pressure.is_none());
        assert!(document.undo());
        assert!(document.sheet().canvas.objects.is_empty());
        assert!(!document.can_undo());
        assert!(document.redo());
        assert_eq!(document.sheet().canvas.objects.len(), 1);
    }

    #[test]
    fn recovered_pen_is_detached_reviewable_and_undoable() {
        let mut document = Document::default();
        let original_scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings {
            color: Color::rgb(10, 20, 30),
            width: 4.0,
            opacity: 0.6,
            band: Band::Below,
            finish: StrokeFinishOptions {
                tolerance: 0.0,
                smooth: false,
            },
            ..InkSettings::default()
        };
        let revision = controller.recovery_revision();
        frame(
            &mut controller,
            &mut document,
            &original_scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[button(pos2(10.0, 10.0), true)],
        );
        assert!(controller.recovery_revision() > revision);
        let revision = controller.recovery_revision();
        frame(
            &mut controller,
            &mut document,
            &original_scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[Event::PointerMoved(pos2(30.0, 20.0))],
        );
        assert!(controller.recovery_revision() > revision);
        let recovered = controller.pending_for_recovery(&document, false).unwrap();
        assert!(recovered.was_held);
        assert!(recovered.source_current);
        assert_eq!(recovered.pointer, RecoveredPointer::Mouse);
        assert_eq!(recovered.samples.len(), 2);

        let mut restarted = Document::new(document.workbook().clone());
        restarted.mark_dirty();
        let mut restored = InkController::default();
        restored
            .restore_recovered_ink(&restarted, &recovered)
            .unwrap();
        assert!(restored.pending_was_recovered());
        assert!(restored.has_released_gesture());
        assert_eq!(restored.pending_settings(), Some(settings));
        assert_eq!(restored.pending_samples(), controller.pending_samples());
        assert_eq!(
            restored
                .pending_for_recovery(&restarted, false)
                .unwrap()
                .pointer,
            recovered.pointer
        );
        assert!(restarted.sheet().canvas.objects.is_empty());
        let restarted_scene = scene(&restarted);
        assert!(matches!(
            restored.retry(&mut restarted, &restarted_scene),
            Ok(InkOutcome::Drawn(_))
        ));
        let object = &restarted.sheet().canvas.objects[0];
        assert_eq!(object.band, Band::Below);
        assert_eq!(object.style.opacity, 0.6);
        assert!(restarted.undo());
        assert!(restarted.sheet().canvas.objects.is_empty());
        assert!(restarted.redo());
        assert_eq!(restarted.sheet().canvas.objects.len(), 1);
    }

    #[test]
    fn recovered_stale_rejected_and_scene_dependent_input_cannot_retry() {
        let mut document = Document::default();
        let original_scene = scene(&document);
        let mut controller = InkController::default();
        frame(
            &mut controller,
            &mut document,
            &original_scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[button(pos2(10.0, 10.0), true)],
        );
        let current = controller.pending_for_recovery(&document, false).unwrap();
        assert!(
            !controller
                .pending_for_recovery(&document, true)
                .unwrap()
                .source_current
        );
        let root = document.sheet().root;
        document
            .edit_sheet("Concurrent", |sheet| sheet.set_title(root, "Changed"))
            .unwrap();
        let stale = controller.pending_for_recovery(&document, false).unwrap();
        assert!(!stale.source_current);
        let mut restored = InkController::default();
        restored.restore_recovered_ink(&document, &stale).unwrap();
        let updated_scene = scene(&document);
        assert_eq!(
            restored.pending_recovery_block(),
            Some(InkRecoveryBlock::StaleDocument)
        );
        assert!(matches!(
            restored.retry(&mut document, &updated_scene),
            Err(InkControllerError::StaleDocument { .. })
        ));
        assert!(restored.pending_samples().len() == 1);
        restored.cancel();

        let mut rejected = current.clone();
        rejected.rejected = true;
        restored
            .restore_recovered_ink(&document, &rejected)
            .unwrap();
        assert!(matches!(
            restored.retry(&mut document, &updated_scene),
            Err(InkControllerError::RejectedInputPending)
        ));
        restored.cancel();

        for mode in [InkMode::ObjectEraser, InkMode::PixelEraser, InkMode::Lasso] {
            let mut path = current.clone();
            path.mode = mode;
            restored.restore_recovered_ink(&document, &path).unwrap();
            assert_eq!(
                restored.pending_recovery_block(),
                Some(InkRecoveryBlock::SceneDependent)
            );
            assert!(matches!(
                restored.retry(&mut document, &updated_scene),
                Err(InkControllerError::RecoveryNeedsReview)
            ));
            assert_eq!(restored.pending_samples().len(), 1);
            restored.cancel();
        }
    }

    #[test]
    fn malformed_recovered_ink_is_rejected_without_replacing_input() {
        let document = Document::default();
        let valid = RecoveredInkGesture {
            mode: InkMode::Pen,
            settings: InkSettings::default().into(),
            pointer: RecoveredPointer::Pen { id: 7 },
            sheet: document.active_index(),
            was_held: true,
            rejected: false,
            source_current: true,
            additive: false,
            samples: vec![InkSample::new(Vec2::ZERO, Some(0.7)).into()],
        };
        let mut cases = Vec::new();
        let mut empty = valid.clone();
        empty.samples.clear();
        cases.push(empty);
        let mut too_many = valid.clone();
        too_many.samples = vec![valid.samples[0]; MAX_INPUT_SAMPLES + 1];
        cases.push(too_many);
        let mut nonfinite = valid.clone();
        nonfinite.samples[0].x_bits = f32::NAN.to_bits();
        cases.push(nonfinite);
        let mut pressure = valid.clone();
        pressure.samples[0].pressure_bits = Some(f32::INFINITY.to_bits());
        cases.push(pressure);
        let mut width = valid.clone();
        width.settings.width_bits = f32::NAN.to_bits();
        cases.push(width);
        let mut opacity = valid.clone();
        opacity.settings.opacity_bits = f32::NAN.to_bits();
        cases.push(opacity);
        let mut sheet = valid.clone();
        sheet.sheet += 1;
        cases.push(sheet);
        let mut controller = InkController::default();
        for case in cases {
            assert!(controller.restore_recovered_ink(&document, &case).is_err());
            assert!(!controller.has_pending_gesture());
        }
        let mut invalid_tolerance = valid.clone();
        invalid_tolerance.settings.tolerance_bits = f32::NAN.to_bits();
        controller
            .restore_recovered_ink(&document, &invalid_tolerance)
            .unwrap();
        assert!(
            controller
                .pending_settings()
                .unwrap()
                .finish
                .tolerance
                .is_nan()
        );
        assert_eq!(
            controller
                .pending_for_recovery(&document, false)
                .unwrap()
                .pointer,
            RecoveredPointer::Pen { id: 7 }
        );
        assert!(controller.restore_recovered_ink(&document, &valid).is_err());
        assert_eq!(controller.pending_samples().len(), 1);
    }

    #[test]
    fn pen_pressure_is_owned_and_committed_with_its_positions() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let points = [pos2(10.0, 10.0), pos2(20.0, 15.0), pos2(30.0, 10.0)];
        for (event, pressure) in [
            (button(points[0], true), 0.2),
            (Event::PointerMoved(points[1]), 0.6),
            (button(points[2], false), 0.1),
        ] {
            let result = pen_frame(
                &mut controller,
                &mut document,
                &scene,
                &[event],
                vec![Some(PenSample { id: 7, pressure })],
                None,
            );
            assert!(result.errors.is_empty());
        }
        let CanvasObjectKind::Ink(ink) = &document.sheet().canvas.objects[0].kind else {
            panic!("expected ink")
        };
        assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.2, 0.6, 0.1]);
        assert_eq!(ink.points.len(), 3);
        assert!(document.undo());
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn touch_pointer_buttons_in_separate_frames_do_not_start_or_end_mouse_ink() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        let end = start + vec2(20.0, 10.0);
        let down = touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(start, true)],
            vec![true],
            None,
        );
        assert!(down.captured && down.errors.is_empty());
        assert!(!controller.has_pending_gesture());
        let touch_start = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[touch(1, TouchPhase::Start, start, Some(0.3))],
        );
        assert!(touch_start.errors.is_empty());
        assert!(controller.has_pending_gesture());
        let up = touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(end, false)],
            vec![true],
            None,
        );
        assert!(up.errors.is_empty());
        assert!(!controller.has_released_gesture());
        let touch_end = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[touch(1, TouchPhase::End, end, Some(0.7))],
        );
        assert!(touch_end.errors.is_empty());
        assert!(matches!(
            touch_end.outcomes.as_slice(),
            [InkOutcome::Drawn(_)]
        ));
        let CanvasObjectKind::Ink(ink) = &document.sheet().canvas.objects[0].kind else {
            panic!("expected touch ink");
        };
        assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.3, 0.7]);
        let mouse = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::Pen,
            InkSettings::default(),
            &[start, end],
        );
        assert!(mouse.errors.is_empty());
        assert_eq!(document.sheet().canvas.objects.len(), 2);
        let CanvasObjectKind::Ink(mouse_ink) = &document.sheet().canvas.objects[1].kind else {
            panic!("expected mouse ink");
        };
        assert!(mouse_ink.pressure.is_none());
    }

    #[test]
    fn ambiguous_touch_pointer_start_is_reported_without_creating_mouse_ink() {
        for issue in [
            TouchPointerIssue::EventMismatch,
            TouchPointerIssue::QueueOverflow,
            TouchPointerIssue::FrameTooLarge,
        ] {
            let mut document = Document::default();
            let scene = scene(&document);
            let mut controller = InkController::default();
            let start = pos2(10.0, 10.0);
            let events = [
                button(start, true),
                Event::PointerMoved(start + vec2(10.0, 0.0)),
                button(start + vec2(20.0, 0.0), false),
            ];
            let result = touch_pointer_frame(
                &mut controller,
                &mut document,
                &scene,
                &events,
                vec![true, false, true],
                Some(issue),
            );
            assert_eq!(
                result.errors,
                [InkControllerError::UncorrelatedTouchInput {
                    issue,
                    position: start
                }]
            );
            assert!(result.captured);
            assert!(!controller.has_pending_gesture());
            assert!(document.sheet().canvas.objects.is_empty());
        }
    }

    #[test]
    fn ambiguous_touch_pointer_batch_cannot_create_a_touch_stroke() {
        for issue in [
            TouchPointerIssue::EventMismatch,
            TouchPointerIssue::QueueOverflow,
            TouchPointerIssue::FrameTooLarge,
        ] {
            let mut document = Document::default();
            let scene = scene(&document);
            let mut controller = InkController::default();
            let start = pos2(10.0, 10.0);
            let end = start + vec2(20.0, 10.0);
            let events = [
                button(start, true),
                touch(1, TouchPhase::Start, start, Some(0.3)),
                touch(1, TouchPhase::Move, end, Some(0.6)),
                button(end, false),
                Event::PointerGone,
                touch(1, TouchPhase::End, end, Some(0.7)),
            ];
            let result = touch_pointer_frame(
                &mut controller,
                &mut document,
                &scene,
                &events,
                Vec::new(),
                Some(issue),
            );
            assert_eq!(
                result.errors,
                [InkControllerError::UncorrelatedTouchInput {
                    issue,
                    position: start
                }]
            );
            assert!(result.captured);
            assert!(!controller.has_pending_gesture());
            assert!(document.sheet().canvas.objects.is_empty());
        }
    }

    #[test]
    fn ambiguous_touch_only_start_stays_blocked_until_release() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        let end = start + vec2(20.0, 10.0);
        let issue = TouchPointerIssue::QueueOverflow;
        let result = touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[touch(1, TouchPhase::Start, start, Some(0.3))],
            Vec::new(),
            Some(issue),
        );
        assert_eq!(
            result.errors,
            [InkControllerError::UncorrelatedTouchInput {
                issue,
                position: start
            }]
        );
        assert!(!controller.has_pending_gesture());
        let release = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[
                touch(1, TouchPhase::Move, end, Some(0.6)),
                touch(1, TouchPhase::End, end, Some(0.7)),
            ],
        );
        assert!(release.errors.is_empty());
        assert!(document.sheet().canvas.objects.is_empty());
        let fresh = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[
                touch(2, TouchPhase::Start, start, Some(0.3)),
                touch(2, TouchPhase::End, end, Some(0.7)),
            ],
        );
        assert!(fresh.errors.is_empty());
        assert!(matches!(fresh.outcomes.as_slice(), [InkOutcome::Drawn(_)]));
        assert_eq!(document.sheet().canvas.objects.len(), 1);
    }

    #[test]
    fn ambiguous_touch_frame_retains_only_previously_accepted_samples() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        let end = start + vec2(20.0, 10.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[touch(1, TouchPhase::Start, start, Some(0.3))],
        );
        let accepted = controller.pending_samples().to_vec();
        let result = touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[
                touch(1, TouchPhase::Move, end, Some(0.6)),
                touch(1, TouchPhase::End, end, Some(0.7)),
            ],
            Vec::new(),
            Some(TouchPointerIssue::EventMismatch),
        );
        assert_eq!(
            result.errors,
            [InkControllerError::Interrupted(
                InkCancelReason::PointerSourceChanged
            )]
        );
        assert!(controller.has_released_gesture());
        assert_eq!(controller.pending_samples(), accepted);
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn ambiguous_pen_frame_cannot_start_touch_ink() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        let issue = PenInputIssue::PointerMismatch;
        let events = [
            touch(1, TouchPhase::Start, start, Some(0.3)),
            touch(1, TouchPhase::End, start + vec2(20.0, 10.0), Some(0.7)),
        ];
        let result = controller.process_frame_with_pen(
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            InkFrameInput {
                events: &events,
                canvas: Rect::from_min_max(pos2(-2_000.0, -2_000.0), pos2(2_000.0, 2_000.0)),
                transform: xf(),
                enabled: true,
                can_start: true,
                modifiers: Modifiers::NONE,
            },
            &PenFrameData {
                issue: Some(issue),
                ..PenFrameData::default()
            },
        );
        assert_eq!(
            result.errors,
            [InkControllerError::UncorrelatedPenInput {
                issue,
                position: start
            }]
        );
        assert!(result.captured);
        assert!(!controller.has_pending_gesture());
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn touch_pointer_interrupts_an_unidentified_mouse_and_preserves_its_samples() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[button(start, true)],
        );
        let up = touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(start + vec2(20.0, 0.0), false)],
            vec![true],
            None,
        );
        assert!(up.errors.is_empty());
        assert!(!controller.has_released_gesture());
        let down = touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(start + vec2(20.0, 0.0), true)],
            vec![true],
            None,
        );
        assert_eq!(
            down.errors,
            [InkControllerError::Interrupted(
                InkCancelReason::PointerSourceChanged
            )]
        );
        assert!(controller.has_released_gesture());
        assert_eq!(controller.pending_samples().len(), 1);
        assert_eq!(controller.pending_samples()[0].pressure, None);
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn confirmed_pen_owns_its_stroke_while_a_touch_contact_is_active() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        let touch_pos = pos2(90.0, 90.0);
        let first = PenSample {
            id: 7,
            pressure: 0.2,
        };
        let last = PenSample {
            id: 7,
            pressure: 0.8,
        };
        pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(start, true)],
            vec![Some(first)],
            None,
        );
        let pointer = touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(touch_pos, true)],
            vec![true],
            None,
        );
        assert!(pointer.errors.is_empty(), "{:?}", pointer.errors);
        let contact = touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[
                button(touch_pos, true),
                touch(1, TouchPhase::Start, touch_pos, Some(0.4)),
            ],
            vec![true, false],
            None,
        );
        assert!(contact.errors.is_empty(), "{:?}", contact.errors);
        let pen_move = pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[Event::PointerMoved(start + vec2(20.0, 10.0))],
            vec![Some(last)],
            None,
        );
        assert!(pen_move.errors.is_empty(), "{:?}", pen_move.errors);
        let release_touch = touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[
                button(touch_pos, false),
                Event::PointerGone,
                touch(1, TouchPhase::End, touch_pos, Some(0.4)),
            ],
            vec![true, true, false],
            None,
        );
        assert!(
            release_touch.errors.is_empty(),
            "{:?}",
            release_touch.errors
        );
        let release_pen = pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(start + vec2(40.0, 20.0), false)],
            vec![Some(last)],
            None,
        );
        assert!(release_pen.errors.is_empty(), "{:?}", release_pen.errors);
        assert!(!controller.has_pending_gesture());
        let CanvasObjectKind::Ink(ink) = &document.sheet().canvas.objects[0].kind else {
            panic!("expected pen ink");
        };
        assert_eq!(ink.points.len(), 3);
        assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.2, 0.8, 0.8]);
    }

    #[test]
    fn a_confirmed_pen_can_start_before_touch_in_the_same_frame() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        let touch_pos = pos2(90.0, 90.0);
        let pen = PenSample {
            id: 7,
            pressure: 0.5,
        };
        let output = pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[
                button(start, true),
                touch(1, TouchPhase::Start, touch_pos, Some(0.4)),
                Event::PointerMoved(start + vec2(20.0, 10.0)),
                touch(1, TouchPhase::End, touch_pos, Some(0.4)),
                button(start + vec2(40.0, 20.0), false),
            ],
            vec![Some(pen), None, Some(pen), None, Some(pen)],
            None,
        );
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert!(!controller.has_pending_gesture());
        assert_eq!(document.sheet().canvas.objects.len(), 1);
        let CanvasObjectKind::Ink(ink) = &document.sheet().canvas.objects[0].kind else {
            panic!("expected pen ink");
        };
        assert_eq!(ink.points.len(), 3);
        assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.5, 0.5, 0.5]);
    }

    #[test]
    fn a_pen_does_not_take_over_a_touch_that_started_first() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let touch_start = pos2(10.0, 10.0);
        let pen_start = pos2(100.0, 100.0);
        let pen = PenSample {
            id: 7,
            pressure: 0.8,
        };
        let output = pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[
                touch(1, TouchPhase::Start, touch_start, Some(0.4)),
                button(pen_start, true),
                Event::PointerMoved(pen_start + vec2(20.0, 20.0)),
                button(pen_start + vec2(40.0, 40.0), false),
                touch(
                    1,
                    TouchPhase::End,
                    touch_start + vec2(30.0, 10.0),
                    Some(0.4),
                ),
            ],
            vec![None, Some(pen), Some(pen), Some(pen), None],
            None,
        );
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(document.sheet().canvas.objects.len(), 1);
        let CanvasObjectKind::Ink(ink) = &document.sheet().canvas.objects[0].kind else {
            panic!("expected touch ink");
        };
        assert_eq!(ink.points.len(), 2);
        assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.4, 0.4]);
    }

    #[test]
    fn a_touch_outliving_the_pen_cannot_start_a_ghost_stroke() {
        let mut document = Document::default();
        let initial_scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        let touch_pos = pos2(90.0, 90.0);
        let pen = PenSample {
            id: 7,
            pressure: 0.5,
        };
        pen_frame(
            &mut controller,
            &mut document,
            &initial_scene,
            &[button(start, true)],
            vec![Some(pen)],
            None,
        );
        touch_pointer_frame(
            &mut controller,
            &mut document,
            &initial_scene,
            &[touch(1, TouchPhase::Start, touch_pos, Some(0.4))],
            vec![false],
            None,
        );
        let release = pen_frame(
            &mut controller,
            &mut document,
            &initial_scene,
            &[button(start + vec2(30.0, 10.0), false)],
            vec![Some(pen)],
            None,
        );
        assert!(release.errors.is_empty());
        assert_eq!(document.sheet().canvas.objects.len(), 1);
        assert!(controller.blocks_touch_navigation());
        let touch_end = touch_pointer_frame(
            &mut controller,
            &mut document,
            &initial_scene,
            &[
                touch(1, TouchPhase::Move, touch_pos + vec2(5.0, 5.0), Some(0.4)),
                touch(1, TouchPhase::End, touch_pos + vec2(5.0, 5.0), Some(0.4)),
            ],
            vec![false, false],
            None,
        );
        assert!(touch_end.captured);
        assert!(touch_end.errors.is_empty());
        assert!(!controller.blocks_touch_navigation());
        assert_eq!(document.sheet().canvas.objects.len(), 1);

        let new_scene = scene(&document);
        let touch_stroke = frame(
            &mut controller,
            &mut document,
            &new_scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[
                touch(2, TouchPhase::Start, touch_pos, Some(0.2)),
                touch(2, TouchPhase::End, touch_pos + vec2(30.0, 10.0), Some(0.6)),
            ],
        );
        assert!(touch_stroke.errors.is_empty(), "{:?}", touch_stroke.errors);
        assert_eq!(document.sheet().canvas.objects.len(), 2);
    }

    #[test]
    fn focus_loss_still_interrupts_a_pen_with_touch_contacts() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(start, true)],
            vec![Some(PenSample {
                id: 7,
                pressure: 0.2,
            })],
            None,
        );
        touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[touch(1, TouchPhase::Start, pos2(90.0, 90.0), Some(0.4))],
            vec![false],
            None,
        );
        let focus = pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[
                touch(1, TouchPhase::Move, pos2(95.0, 95.0), Some(0.4)),
                Event::PointerMoved(start + vec2(20.0, 10.0)),
                Event::WindowFocused(false),
            ],
            vec![
                None,
                Some(PenSample {
                    id: 7,
                    pressure: 0.6,
                }),
                None,
            ],
            None,
        );
        assert_eq!(
            focus.errors,
            [InkControllerError::Interrupted(InkCancelReason::FocusLost)]
        );
        assert!(controller.has_released_gesture());
        assert_eq!(controller.pending_samples().len(), 2);
        assert!(!controller.blocks_touch_navigation());
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn multiple_suppressed_contacts_can_cancel_without_swallowing_the_next_mouse_stroke() {
        let mut document = Document::default();
        let initial_scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        let pen = PenSample {
            id: 7,
            pressure: 0.5,
        };
        pen_frame(
            &mut controller,
            &mut document,
            &initial_scene,
            &[button(start, true)],
            vec![Some(pen)],
            None,
        );
        touch_pointer_frame(
            &mut controller,
            &mut document,
            &initial_scene,
            &[touch(1, TouchPhase::Start, pos2(90.0, 90.0), Some(0.4))],
            vec![false],
            None,
        );
        pen_frame(
            &mut controller,
            &mut document,
            &initial_scene,
            &[button(start + vec2(30.0, 10.0), false)],
            vec![Some(pen)],
            None,
        );
        let contacts = touch_pointer_frame(
            &mut controller,
            &mut document,
            &initial_scene,
            &[
                touch(2, TouchPhase::Start, pos2(120.0, 90.0), Some(0.4)),
                touch(1, TouchPhase::Cancel, pos2(90.0, 90.0), Some(0.4)),
                touch(2, TouchPhase::Cancel, pos2(120.0, 90.0), Some(0.4)),
            ],
            vec![false, false, false],
            None,
        );
        assert!(contacts.captured);
        assert!(contacts.errors.is_empty(), "{:?}", contacts.errors);
        assert!(!controller.blocks_touch_navigation());
        assert_eq!(document.sheet().canvas.objects.len(), 1);

        let new_scene = scene(&document);
        let mouse = frame(
            &mut controller,
            &mut document,
            &new_scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[
                button(pos2(200.0, 200.0), true),
                Event::PointerMoved(pos2(220.0, 210.0)),
                button(pos2(230.0, 210.0), false),
            ],
        );
        assert!(mouse.errors.is_empty(), "{:?}", mouse.errors);
        assert_eq!(document.sheet().canvas.objects.len(), 2);
    }

    #[test]
    fn tool_change_retains_confirmed_pen_movement_batched_with_touch() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(start, true)],
            vec![Some(PenSample {
                id: 7,
                pressure: 0.2,
            })],
            None,
        );
        touch_pointer_frame(
            &mut controller,
            &mut document,
            &scene,
            &[touch(1, TouchPhase::Start, pos2(90.0, 90.0), Some(0.4))],
            vec![false],
            None,
        );
        let events = [
            touch(1, TouchPhase::Move, pos2(95.0, 95.0), Some(0.4)),
            Event::PointerMoved(start + vec2(20.0, 10.0)),
        ];
        let changed = controller.process_frame_with_pen(
            &mut document,
            &scene,
            Some(InkMode::Highlighter),
            InkSettings::default(),
            InkFrameInput {
                events: &events,
                canvas: Rect::from_min_max(pos2(-2_000.0, -2_000.0), pos2(2_000.0, 2_000.0)),
                transform: xf(),
                enabled: true,
                can_start: true,
                modifiers: Modifiers::NONE,
            },
            &PenFrameData {
                samples: vec![
                    None,
                    Some(PenSample {
                        id: 7,
                        pressure: 0.6,
                    }),
                ],
                ..PenFrameData::default()
            },
        );
        assert_eq!(
            changed.errors,
            [InkControllerError::Interrupted(
                InkCancelReason::ToolChanged
            )]
        );
        assert_eq!(controller.pending_samples().len(), 2);
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn suspending_ink_does_not_sample_or_release_a_touch_pointer_as_mouse_input() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[button(start, true)],
        );
        let release = [button(start + vec2(30.0, 0.0), false)];
        let output = controller.suspend_with_pen(
            &document,
            xf(),
            &release,
            &PenFrameData {
                touch_pointers: vec![true],
                ..PenFrameData::default()
            },
        );
        assert_eq!(
            output.errors,
            [InkControllerError::Interrupted(InkCancelReason::Disabled)]
        );
        assert_eq!(controller.pending_samples().len(), 1);
        assert!(controller.mouse_down);
        controller.suspend(&document, xf(), &release);
        assert!(!controller.mouse_down);
    }

    #[test]
    fn unmatched_initial_pen_input_is_rejected_without_creating_mouse_ink() {
        for issue in [
            PenInputIssue::PointerMismatch,
            PenInputIssue::QueueOverflow,
            PenInputIssue::FrameTooLarge,
        ] {
            let mut document = Document::default();
            let scene = scene(&document);
            let mut controller = InkController::default();
            let start = pos2(10.0, 10.0);
            let end = start + vec2(20.0, 10.0);
            let events = [
                button(start, true),
                Event::PointerMoved(end),
                button(end, false),
            ];
            let rejected = controller.process_frame_with_pen(
                &mut document,
                &scene,
                Some(InkMode::Pen),
                InkSettings::default(),
                InkFrameInput {
                    events: &events,
                    canvas: Rect::from_min_max(pos2(-100.0, -100.0), pos2(100.0, 100.0)),
                    transform: xf(),
                    enabled: true,
                    can_start: true,
                    modifiers: Modifiers::NONE,
                },
                &PenFrameData {
                    issue: Some(issue),
                    ..PenFrameData::default()
                },
            );
            assert!(rejected.captured);
            assert_eq!(
                rejected.errors,
                [InkControllerError::UncorrelatedPenInput {
                    issue,
                    position: start
                }]
            );
            assert!(document.sheet().canvas.objects.is_empty());
            assert!(!controller.has_pending_gesture());

            let mouse = mouse_gesture(
                &mut controller,
                &mut document,
                &scene,
                InkMode::Pen,
                InkSettings::default(),
                &[start, end],
            );
            assert!(mouse.errors.is_empty());
            let CanvasObjectKind::Ink(ink) = &document.sheet().canvas.objects[0].kind else {
                panic!("expected mouse ink");
            };
            assert!(ink.pressure.is_none());
        }
    }

    #[test]
    fn unmatched_pen_release_retains_only_the_previously_correlated_samples() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(start, true)],
            vec![Some(PenSample {
                id: 7,
                pressure: 0.2,
            })],
            None,
        );
        let events = [button(start + vec2(20.0, 10.0), false)];
        let result = controller.process_frame_with_pen(
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            InkFrameInput {
                events: &events,
                canvas: Rect::from_min_max(pos2(-100.0, -100.0), pos2(100.0, 100.0)),
                transform: xf(),
                enabled: true,
                can_start: true,
                modifiers: Modifiers::NONE,
            },
            &PenFrameData {
                canceled: Some(7),
                issue: Some(PenInputIssue::PointerMismatch),
                ..PenFrameData::default()
            },
        );
        assert_eq!(
            result.errors,
            [InkControllerError::Interrupted(
                InkCancelReason::PointerSourceChanged
            )]
        );
        assert!(controller.has_released_gesture());
        assert_eq!(controller.pending_samples().len(), 1);
        assert_eq!(controller.pending_samples()[0].pressure, Some(0.2));
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn conflicting_or_canceled_pen_input_retains_the_original_samples() {
        for other in [
            None,
            Some(PenSample {
                id: 8,
                pressure: 0.8,
            }),
        ] {
            let mut document = Document::default();
            let scene = scene(&document);
            let mut controller = InkController::default();
            let start = pos2(10.0, 10.0);
            pen_frame(
                &mut controller,
                &mut document,
                &scene,
                &[button(start, true)],
                vec![Some(PenSample {
                    id: 7,
                    pressure: 0.2,
                })],
                None,
            );
            let result = pen_frame(
                &mut controller,
                &mut document,
                &scene,
                &[Event::PointerMoved(start + vec2(5.0, 0.0))],
                vec![other],
                Some(7),
            );
            assert_eq!(
                result.errors,
                [InkControllerError::Interrupted(
                    InkCancelReason::PointerSourceChanged
                )]
            );
            assert_eq!(controller.pending_samples().len(), 1);
            assert_eq!(controller.pending_samples()[0].pressure, Some(0.2));
            assert!(document.sheet().canvas.objects.is_empty());
            assert!(controller.cancel());
            let mouse = mouse_gesture(
                &mut controller,
                &mut document,
                &scene,
                InkMode::Pen,
                InkSettings::default(),
                &[start, start + vec2(10.0, 0.0)],
            );
            assert!(mouse.errors.is_empty());
            let CanvasObjectKind::Ink(ink) = &document.sheet().canvas.objects[0].kind else {
                panic!("expected mouse ink")
            };
            assert!(ink.pressure.is_none());
        }
    }

    #[test]
    fn pen_cancellation_and_invalid_pressure_never_commit_partial_input() {
        let start = pos2(10.0, 10.0);
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(start, true)],
            vec![Some(PenSample {
                id: 7,
                pressure: 0.2,
            })],
            None,
        );
        let invalid = pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[Event::PointerMoved(start + vec2(5.0, 0.0))],
            vec![Some(PenSample {
                id: 7,
                pressure: 1.5,
            })],
            None,
        );
        assert!(matches!(
            invalid.errors.as_slice(),
            [InkControllerError::InvalidSample(RejectedSample {
                reason: crate::ink_input::SampleRejection::InvalidPressure,
                ..
            })]
        ));
        let canceled = pen_frame(&mut controller, &mut document, &scene, &[], vec![], Some(7));
        assert_eq!(
            canceled.errors,
            [InkControllerError::Interrupted(
                InkCancelReason::PointerGone
            )]
        );
        assert_eq!(controller.pending_samples().len(), 1);
        assert!(controller.pending_has_rejected_input());
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn suspension_retains_matched_pen_samples_before_interrupting() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let start = pos2(10.0, 10.0);
        pen_frame(
            &mut controller,
            &mut document,
            &scene,
            &[button(start, true)],
            vec![Some(PenSample {
                id: 7,
                pressure: 0.2,
            })],
            None,
        );
        let events = [
            Event::PointerMoved(start + vec2(5.0, 0.0)),
            button(start + vec2(10.0, 0.0), false),
        ];
        let output = controller.suspend_with_pen(
            &document,
            xf(),
            &events,
            &PenFrameData {
                samples: vec![
                    Some(PenSample {
                        id: 7,
                        pressure: 0.6,
                    }),
                    Some(PenSample {
                        id: 7,
                        pressure: 0.9,
                    }),
                ],
                ..PenFrameData::default()
            },
        );
        assert_eq!(
            output.errors,
            [InkControllerError::Interrupted(InkCancelReason::Disabled)]
        );
        assert_eq!(
            controller
                .pending_samples()
                .iter()
                .map(|sample| sample.pressure)
                .collect::<Vec<_>>(),
            [Some(0.2), Some(0.6), Some(0.9)]
        );
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn touch_pressure_and_synthetic_mouse_create_exactly_one_highlighter() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings {
            finish: StrokeFinishOptions {
                tolerance: 0.0,
                smooth: false,
            },
            ..InkSettings::default()
        };
        let start = pos2(100.0, 100.0);
        let middle = pos2(110.0, 105.0);
        let end = pos2(120.0, 100.0);
        let first = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Highlighter),
            settings,
            xf(),
            &[
                Event::PointerMoved(start),
                touch(1, TouchPhase::Start, start, Some(0.2)),
                button(start, true),
            ],
        );
        assert!(first.errors.is_empty());
        let second = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Highlighter),
            settings,
            xf(),
            &[
                Event::PointerMoved(middle),
                touch(1, TouchPhase::Move, middle, Some(0.6)),
            ],
        );
        assert!(second.errors.is_empty());
        assert_eq!(controller.pending_samples().len(), 2);
        let last = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Highlighter),
            settings,
            xf(),
            &[
                Event::PointerMoved(end),
                touch(1, TouchPhase::End, end, Some(0.9)),
                button(end, false),
            ],
        );
        assert!(last.errors.is_empty());
        assert!(matches!(last.outcomes.as_slice(), [InkOutcome::Drawn(_)]));
        assert_eq!(document.sheet().canvas.objects.len(), 1);
        let CanvasObjectKind::Ink(ink) = &document.sheet().canvas.objects[0].kind else {
            panic!("expected ink")
        };
        assert_eq!(ink.tool, InkTool::Highlighter);
        assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.2, 0.6, 0.9]);
        assert_eq!(ink.points.len(), 3);
        assert!(document.undo());
        assert!(!document.can_undo());
    }

    #[test]
    fn multitouch_and_focus_loss_retain_without_a_delayed_mouse_stroke() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings::default();
        let p = pos2(100.0, 100.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::Start, p, None), button(p, true)],
        );
        let second = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(2, TouchPhase::Start, p + vec2(20.0, 0.0), None)],
        );
        assert_eq!(
            second.errors,
            vec![InkControllerError::Interrupted(InkCancelReason::Multitouch)]
        );
        assert!(controller.has_released_gesture());
        assert_eq!(controller.pending_samples().len(), 1);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::End, p, None), button(p, false)],
        );
        let end = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(2, TouchPhase::End, p + vec2(20.0, 0.0), None)],
        );
        assert!(end.outcomes.is_empty());
        assert!(document.sheet().canvas.objects.is_empty());
        assert!(controller.cancel());
        let canceled = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[button(p, true), Event::WindowFocused(false)],
        );
        assert_eq!(
            canceled.errors,
            vec![InkControllerError::Interrupted(InkCancelReason::FocusLost)]
        );
        assert!(controller.has_released_gesture());
        assert_eq!(controller.pending_samples().len(), 1);
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn interrupted_tool_or_disabled_input_retains_the_last_owned_samples_for_retry() {
        for changed_tool in [false, true] {
            let mut document = Document::default();
            let scene = scene(&document);
            let mut controller = InkController::default();
            let settings = InkSettings::default();
            let p = pos2(100.0, 100.0);
            frame(
                &mut controller,
                &mut document,
                &scene,
                Some(InkMode::Pen),
                settings,
                xf(),
                &[button(p, true)],
            );
            let end = p + vec2(30.0, 15.0);
            let events = [Event::PointerMoved(p + vec2(10.0, 5.0)), button(end, false)];
            let interrupted = controller.process_frame(
                &mut document,
                &scene,
                if changed_tool {
                    Some(InkMode::Lasso)
                } else {
                    Some(InkMode::Pen)
                },
                settings,
                InkFrameInput {
                    events: &events,
                    canvas: Rect::from_min_max(pos2(-2_000.0, -2_000.0), pos2(2_000.0, 2_000.0)),
                    transform: xf(),
                    enabled: changed_tool,
                    can_start: false,
                    modifiers: Modifiers::NONE,
                },
            );
            let reason = if changed_tool {
                InkCancelReason::ToolChanged
            } else {
                InkCancelReason::Disabled
            };
            assert_eq!(
                interrupted.errors,
                vec![InkControllerError::Interrupted(reason)]
            );
            assert!(controller.has_released_gesture());
            assert_eq!(
                controller.pending_samples().last().unwrap().position,
                Vec2::new(end.x, end.y)
            );
            assert!(document.sheet().canvas.objects.is_empty());
            assert!(matches!(
                controller.retry(&mut document, &scene),
                Ok(InkOutcome::Drawn(_))
            ));
            assert!(!controller.has_pending_gesture());
        }
    }

    #[test]
    fn suspended_multitouch_stops_sampling_at_the_second_contact_and_tracks_releases() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings::default();
        let p = pos2(100.0, 100.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::Start, p, Some(0.1))],
        );
        let out = controller.suspend(
            &document,
            xf(),
            &[
                touch(1, TouchPhase::Move, p + vec2(5.0, 0.0), Some(0.2)),
                touch(2, TouchPhase::Start, p + vec2(20.0, 0.0), None),
                touch(1, TouchPhase::Move, p + vec2(10.0, 0.0), Some(0.3)),
                touch(1, TouchPhase::End, p + vec2(10.0, 0.0), Some(0.4)),
                touch(2, TouchPhase::End, p + vec2(20.0, 0.0), None),
            ],
        );
        assert_eq!(
            out.errors,
            vec![InkControllerError::Interrupted(InkCancelReason::Disabled)]
        );
        assert_eq!(controller.pending_samples().len(), 2);
        assert_eq!(controller.pending_samples()[1].pressure, Some(0.2));
        assert!(controller.cancel());
        assert!(!controller.needs_input_tracking());
        let next = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(3, TouchPhase::Start, p, Some(0.5))],
        );
        assert!(next.errors.is_empty());
        assert!(controller.has_pending_gesture());
    }

    #[test]
    fn rejected_samples_and_changed_document_never_commit_partial_input() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings::default();
        let p = pos2(100.0, 100.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::Start, p, Some(0.2))],
        );
        let invalid = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::Move, p + vec2(2.0, 0.0), Some(1.5))],
        );
        assert!(matches!(
            invalid.errors.as_slice(),
            [InkControllerError::InvalidSample(RejectedSample {
                sample: InkSample {
                    pressure: Some(1.5),
                    ..
                },
                ..
            })]
        ));
        let end = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::End, p + vec2(4.0, 0.0), None)],
        );
        assert_eq!(end.errors, vec![InkControllerError::RejectedInputPending]);
        assert_eq!(controller.pending_samples().len(), 2);
        assert!(document.sheet().canvas.objects.is_empty());
        assert!(!document.can_undo());
        assert!(controller.cancel());

        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[button(p, true)],
        );
        document.add_canvas_object(stroke(&[(400.0, 0.0)])).unwrap();
        let before = document.sheet().clone();
        let end = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[button(p + vec2(2.0, 0.0), false)],
        );
        assert!(
            end.errors
                .iter()
                .any(|error| matches!(error, InkControllerError::StaleDocument { .. }))
        );
        assert_eq!(*document.sheet(), before);
        assert!(controller.has_pending_gesture());
        assert!(controller.cancel());
    }

    #[test]
    fn camera_change_rejects_the_gesture_without_losing_accepted_samples() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings::default();
        let start = pos2(100.0, 100.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[button(start, true)],
        );
        let moved = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            Xf {
                origin: pos2(20.0, 0.0),
                ..xf()
            },
            &[Event::PointerMoved(start + vec2(3.0, 0.0))],
        );
        assert!(matches!(
            moved.errors.as_slice(),
            [InkControllerError::CameraChanged { sample: Some(_) }]
        ));
        let released = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[button(start + vec2(4.0, 0.0), false)],
        );
        assert_eq!(
            released.errors,
            vec![InkControllerError::RejectedInputPending]
        );
        assert_eq!(controller.pending_samples().len(), 2);
        assert!(document.sheet().canvas.objects.is_empty());
        assert!(!document.can_undo());
        assert!(controller.cancel());
        let recovered = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::Pen,
            settings,
            &[start, start + vec2(5.0, 0.0)],
        );
        assert!(matches!(
            recovered.outcomes.as_slice(),
            [InkOutcome::Drawn(_)]
        ));
        assert!(recovered.errors.is_empty());
    }

    #[test]
    fn invalid_touch_pressure_rejects_eraser_and_lasso_paths() {
        for mode in [InkMode::ObjectEraser, InkMode::PixelEraser, InkMode::Lasso] {
            let mut document = Document::default();
            let scene = scene(&document);
            let mut controller = InkController::default();
            let settings = InkSettings::default();
            let p = pos2(100.0, 100.0);
            frame(
                &mut controller,
                &mut document,
                &scene,
                Some(mode),
                settings,
                xf(),
                &[touch(1, TouchPhase::Start, p, Some(0.5))],
            );
            let bad = frame(
                &mut controller,
                &mut document,
                &scene,
                Some(mode),
                settings,
                xf(),
                &[touch(1, TouchPhase::Move, p + vec2(1.0, 0.0), Some(1.5))],
            );
            assert!(matches!(
                bad.errors.as_slice(),
                [InkControllerError::InvalidPathSample(InkSample {
                    pressure: Some(1.5),
                    ..
                })]
            ));
            let end = frame(
                &mut controller,
                &mut document,
                &scene,
                Some(mode),
                settings,
                xf(),
                &[touch(1, TouchPhase::End, p + vec2(2.0, 0.0), Some(0.5))],
            );
            assert_eq!(end.errors, vec![InkControllerError::RejectedInputPending]);
            assert_eq!(controller.pending_samples().len(), 2);
            assert!(document.sheet().canvas.objects.is_empty());
            assert!(!document.can_undo());
            assert!(controller.cancel());
        }
    }

    #[test]
    fn invalid_transform_reports_the_raw_touch_position_and_pressure() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let p = pos2(100.0, 100.0);
        let out = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            Xf { zoom: 0.0, ..xf() },
            &[touch(1, TouchPhase::Start, p, Some(0.75))],
        );
        assert_eq!(
            out.errors,
            vec![InkControllerError::InvalidTransform {
                position: p,
                pressure: Some(0.75)
            }]
        );
        assert!(!controller.has_pending_gesture());
        assert!(document.sheet().canvas.objects.is_empty());
    }

    #[test]
    fn nonfinite_pointer_start_is_reported_instead_of_ignored() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let out = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            InkSettings::default(),
            xf(),
            &[touch(
                1,
                TouchPhase::Start,
                pos2(f32::NAN, 100.0),
                Some(0.7),
            )],
        );
        assert!(out.captured);
        assert!(matches!(
            out.errors.as_slice(),
            [InkControllerError::InvalidSample(RejectedSample {
                sample,
                reason: crate::ink_input::SampleRejection::InvalidPosition,
            })] if sample.position.x.is_nan() && sample.pressure == Some(0.7)
        ));
        assert!(document.sheet().canvas.objects.is_empty());
        assert!(!controller.has_pending_gesture());
    }

    #[test]
    fn failed_finish_retains_samples_and_reports_a_blocked_new_start() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings {
            finish: StrokeFinishOptions {
                tolerance: -1.0,
                smooth: false,
            },
            ..InkSettings::default()
        };
        let p = pos2(100.0, 100.0);
        let failed = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::Pen,
            settings,
            &[p, p + vec2(5.0, 0.0)],
        );
        assert!(matches!(
            failed.errors.as_slice(),
            [InkControllerError::Ink(InkInputError::InvalidTolerance(
                -1.0
            ))]
        ));
        let previous = controller.pending_samples().to_vec();
        let other = p + vec2(20.0, 0.0);
        let blocked = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::Start, other, Some(0.7))],
        );
        assert_eq!(
            blocked.errors,
            vec![InkControllerError::StartWhilePending {
                position: other,
                pressure: Some(0.7),
            }]
        );
        assert_eq!(controller.pending_samples(), previous);
        assert!(document.sheet().canvas.objects.is_empty());
        assert!(!document.can_undo());
        assert!(controller.cancel());
    }

    #[test]
    fn failed_finish_can_be_retried_with_new_simplification_options() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings {
            finish: StrokeFinishOptions {
                tolerance: -1.0,
                smooth: false,
            },
            ..InkSettings::default()
        };
        let p = pos2(100.0, 100.0);
        let failed = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::Pen,
            settings,
            &[p, p + vec2(5.0, 0.0)],
        );
        assert!(matches!(
            failed.errors.as_slice(),
            [InkControllerError::Ink(InkInputError::InvalidTolerance(
                -1.0
            ))]
        ));
        let samples = controller.pending_samples().to_vec();
        assert!(
            controller
                .retry_with_finish_options(&mut document, &scene, settings.finish)
                .is_err()
        );
        assert_eq!(controller.pending_samples(), samples);
        let result = controller
            .retry_with_finish_options(&mut document, &scene, StrokeFinishOptions::default())
            .unwrap();
        assert!(matches!(result, InkOutcome::Drawn(_)));
        assert!(!controller.has_pending_gesture());
        assert_eq!(document.sheet().canvas.objects.len(), 1);
        assert!(document.undo());
        assert!(document.sheet().canvas.objects.is_empty());
        assert!(!document.can_undo());
    }

    #[test]
    fn failed_gesture_survives_focus_tool_and_multitouch_changes() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings {
            finish: StrokeFinishOptions {
                tolerance: -1.0,
                smooth: false,
            },
            ..InkSettings::default()
        };
        let p = pos2(100.0, 100.0);
        mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::Pen,
            settings,
            &[p, p + vec2(5.0, 0.0)],
        );
        let previous = controller.pending_samples().to_vec();
        let disabled = controller.process_frame(
            &mut document,
            &scene,
            None,
            settings,
            InkFrameInput {
                events: &[Event::WindowFocused(false)],
                canvas: Rect::from_min_max(pos2(-2000.0, -2000.0), pos2(2000.0, 2000.0)),
                transform: xf(),
                enabled: false,
                can_start: false,
                modifiers: Modifiers::NONE,
            },
        );
        assert!(disabled.outcomes.is_empty());
        assert_eq!(controller.pending_samples(), previous);
        let multiple = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Highlighter),
            settings,
            xf(),
            &[
                touch(1, TouchPhase::Start, p, Some(0.5)),
                touch(2, TouchPhase::Start, p + vec2(1.0, 0.0), Some(0.5)),
            ],
        );
        assert!(matches!(
            multiple.errors.as_slice(),
            [InkControllerError::StartWhilePending { .. }]
        ));
        assert!(multiple.outcomes.is_empty());
        assert_eq!(controller.pending_samples(), previous);
        assert!(controller.has_pending_gesture());
        assert!(document.sheet().canvas.objects.is_empty());
        assert!(controller.cancel());
    }

    #[test]
    fn touch_cancel_retains_input_until_explicit_discard() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings::default();
        let p = pos2(100.0, 100.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::Start, p, Some(0.5)), button(p, true)],
        );
        let canceled = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::Cancel, p, None), button(p, false)],
        );
        assert_eq!(
            canceled.errors,
            vec![InkControllerError::Interrupted(
                InkCancelReason::TouchCanceled
            )]
        );
        assert!(controller.has_released_gesture());
        assert!(controller.cancel());
        assert!(document.sheet().canvas.objects.is_empty());
        let next = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[
                touch(2, TouchPhase::Start, p, Some(0.3)),
                touch(2, TouchPhase::End, p + vec2(2.0, 0.0), Some(0.7)),
            ],
        );
        assert!(next.errors.is_empty());
        assert!(matches!(next.outcomes.as_slice(), [InkOutcome::Drawn(_)]));
    }

    #[test]
    fn canceled_touch_without_synthetic_release_does_not_block_mouse() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings::default();
        let p = pos2(100.0, 100.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::Start, p, None), button(p, true)],
        );
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::Cancel, p, None)],
        );
        assert!(controller.has_released_gesture());
        assert!(controller.cancel());
        let next = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::Pen,
            settings,
            &[p, p + vec2(3.0, 0.0)],
        );
        assert!(next.errors.is_empty());
        assert!(matches!(next.outcomes.as_slice(), [InkOutcome::Drawn(_)]));
    }

    #[test]
    fn non_ink_touch_does_not_capture_a_preceding_synthetic_mouse_press() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings::default();
        let p = pos2(100.0, 100.0);
        for events in [
            vec![button(p, true)],
            vec![touch(1, TouchPhase::Start, p, None)],
            vec![Event::PointerMoved(p + vec2(1.0, 0.0))],
            vec![touch(1, TouchPhase::End, p, None), button(p, false)],
        ] {
            let output = frame(
                &mut controller,
                &mut document,
                &scene,
                None,
                settings,
                xf(),
                &events,
            );
            assert!(!output.captured, "non-ink events claimed: {events:?}");
            assert!(output.errors.is_empty());
        }
        assert!(!controller.needs_input_tracking());
        assert!(!document.can_undo());
    }

    #[test]
    fn canceled_ink_mouse_stays_captured_after_touch_interruption() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings::default();
        let p = pos2(100.0, 100.0);
        let down = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[button(p, true)],
        );
        assert!(down.captured);
        let interrupted = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::Start, p, None)],
        );
        assert_eq!(
            interrupted.errors,
            vec![InkControllerError::Interrupted(InkCancelReason::Multitouch)]
        );
        assert!(controller.cancel());
        let release = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[touch(1, TouchPhase::End, p, None), button(p, false)],
        );
        assert!(release.captured);
        assert!(!controller.needs_input_tracking());
        assert!(!document.can_undo());
    }

    #[test]
    fn pointer_gone_without_release_does_not_block_the_next_mouse_gesture() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings::default();
        let p = pos2(100.0, 100.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[button(p, true)],
        );
        let gone = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[Event::PointerGone],
        );
        assert_eq!(
            gone.errors,
            vec![InkControllerError::Interrupted(
                InkCancelReason::PointerGone
            )]
        );
        assert!(controller.has_released_gesture());
        assert!(controller.cancel());
        let next = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::Pen,
            settings,
            &[p, p + vec2(3.0, 0.0)],
        );
        assert!(next.errors.is_empty());
        assert!(matches!(next.outcomes.as_slice(), [InkOutcome::Drawn(_)]));
    }

    #[test]
    fn canceled_mouse_release_stays_captured_without_swallowing_a_new_click() {
        let mut document = Document::default();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings::default();
        let p = pos2(100.0, 100.0);
        frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[button(p, true)],
        );
        let canceled = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert_eq!(
            canceled.outcomes,
            vec![InkOutcome::Canceled(InkCancelReason::Escape)]
        );
        let next = frame(
            &mut controller,
            &mut document,
            &scene,
            Some(InkMode::Pen),
            settings,
            xf(),
            &[button(p, false), button(p, true), button(p, false)],
        );
        assert!(next.captured);
        assert!(next.errors.is_empty());
        assert!(matches!(next.outcomes.as_slice(), [InkOutcome::Drawn(_)]));
    }

    #[test]
    fn object_eraser_checks_locks_and_deletes_all_hits_in_one_edit() {
        let mut document = Document::default();
        let a = document
            .add_canvas_object(stroke(&[(100.0, 100.0), (110.0, 100.0)]))
            .unwrap();
        let b = document
            .add_canvas_object(stroke(&[(120.0, 100.0), (130.0, 100.0)]))
            .unwrap();
        document.set_canvas_objects_locked(&[b], true).unwrap();
        let before = document.sheet().clone();
        let locked_scene = scene(&document);
        let mut controller = InkController::default();
        let out = mouse_gesture(
            &mut controller,
            &mut document,
            &locked_scene,
            InkMode::ObjectEraser,
            InkSettings::default(),
            &[pos2(105.0, 100.0), pos2(125.0, 100.0)],
        );
        assert!(
            matches!(out.errors.as_slice(), [InkControllerError::Locked(members)] if members.contains(&BoardMember::Object(b)))
        );
        assert_eq!(*document.sheet(), before);
        assert!(controller.has_pending_gesture());
        controller.cancel();
        document.set_canvas_objects_locked(&[b], false).unwrap();
        let before = document.sheet().clone();
        let scene = scene(&document);
        let out = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::ObjectEraser,
            InkSettings::default(),
            &[pos2(105.0, 100.0), pos2(125.0, 100.0)],
        );
        assert!(out.errors.is_empty());
        assert_eq!(out.outcomes, vec![InkOutcome::ObjectErased(vec![a, b])]);
        assert!(document.sheet().canvas.objects.is_empty());
        assert_eq!(document.undo_label(), Some("Erase ink"));
        assert!(document.undo());
        assert_eq!(*document.sheet(), before);
    }

    #[test]
    fn pixel_erase_preserves_group_metadata_order_and_connector_semantics() {
        let mut document = Document::default();
        let mut line = stroke(&[(100.0, 100.0), (110.0, 100.0)]);
        if let CanvasObjectKind::Ink(ink) = &mut line.kind {
            ink.pressure = Some(vec![0.0, 1.0].into());
        }
        line.band = Band::Below;
        line.style.opacity = 0.4;
        line.alt_text = "important".into();
        let a = document.add_canvas_object(line).unwrap();
        let dot = document
            .add_canvas_object(stroke(&[(105.0, 100.0)]))
            .unwrap();
        let group = document.group_canvas_objects(&[a, dot]).unwrap();
        let connector = document
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Object(dot),
                Endpoint::Point(Vec2::new(140.0, 100.0)),
            )))
            .unwrap();
        let before = document.sheet().clone();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings {
            eraser_radius: 1.0,
            ..InkSettings::default()
        };
        let out = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::PixelEraser,
            settings,
            &[pos2(105.0, 100.0)],
        );
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        let [
            InkOutcome::PixelErased {
                originals,
                fragments,
                replacements,
            },
        ] = out.outcomes.as_slice()
        else {
            panic!("expected pixel erase")
        };
        assert_eq!(originals, &[a, dot]);
        assert_eq!(fragments.len(), 2);
        assert_eq!(fragments[0], a);
        assert_eq!(
            replacements,
            &[
                InkReplacement {
                    original: a,
                    fragments: fragments.clone(),
                },
                InkReplacement {
                    original: dot,
                    fragments: Vec::new(),
                },
            ]
        );
        let objects = &document.sheet().canvas.objects;
        assert_eq!(
            objects.iter().map(|object| object.id).collect::<Vec<_>>(),
            vec![a, fragments[1], connector]
        );
        for object in &objects[..2] {
            assert_eq!(object.group, Some(group));
            assert_eq!(object.band, Band::Below);
            assert_eq!(object.style.opacity, 0.4);
            assert_eq!(object.alt_text, "important");
            assert_eq!(object.rotation, 0.0);
        }
        let CanvasObjectKind::Ink(left) = &objects[0].kind else {
            panic!("expected ink")
        };
        let CanvasObjectKind::Ink(right) = &objects[1].kind else {
            panic!("expected ink")
        };
        assert_eq!(&**left.pressure.as_ref().unwrap(), &[0.0, 0.3]);
        assert_eq!(&**right.pressure.as_ref().unwrap(), &[0.7, 1.0]);
        let CanvasObjectKind::Connector(c) = &objects[2].kind else {
            panic!("expected connector")
        };
        assert_eq!(c.from, Endpoint::Point(Vec2::new(105.0, 100.0)));
        assert_eq!(document.undo_label(), Some("Pixel erase"));
        assert!(document.undo());
        assert_eq!(*document.sheet(), before);
        assert!(document.redo());
        assert_eq!(document.sheet().canvas.objects.len(), 3);
    }

    #[test]
    fn pixel_erase_is_atomic_when_a_later_hit_is_locked() {
        let mut document = Document::default();
        let a = document
            .add_canvas_object(stroke(&[(100.0, 100.0), (110.0, 100.0)]))
            .unwrap();
        let b = document
            .add_canvas_object(stroke(&[(120.0, 100.0), (130.0, 100.0)]))
            .unwrap();
        document.set_canvas_objects_locked(&[b], true).unwrap();
        let before = document.sheet().clone();
        let previous_undo = document.undo_label().map(str::to_owned);
        let scene = scene(&document);
        let mut controller = InkController::default();
        let out = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::PixelEraser,
            InkSettings {
                eraser_radius: 1.0,
                ..InkSettings::default()
            },
            &[pos2(105.0, 100.0), pos2(125.0, 100.0)],
        );
        assert!(matches!(
            out.errors.as_slice(),
            [InkControllerError::Locked(members)] if members.contains(&BoardMember::Object(b))
        ));
        assert!(document.sheet().canvas.object(a).is_some());
        assert_eq!(*document.sheet(), before);
        assert_eq!(document.undo_label(), previous_undo.as_deref());
        assert!(controller.has_pending_gesture());
    }

    #[test]
    fn lasso_expands_mixed_groups_and_can_select_locked_members() {
        let mut document = Document::default();
        let ink = document
            .add_canvas_object(stroke(&[(1000.0, 0.0), (1010.0, 0.0)]))
            .unwrap();
        let card = document
            .add_card(
                "Card",
                Vec2::new(1200.0, 0.0),
                Card::new(CardKind::Sticky, Vec2::new(100.0, 80.0)),
            )
            .unwrap();
        document
            .group_board_members(&[BoardMember::Object(ink), BoardMember::Card(card)])
            .unwrap();
        document
            .set_board_members_locked(&[BoardMember::Card(card)], true)
            .unwrap();
        let before = document.sheet().clone();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let out = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::Lasso,
            InkSettings::default(),
            &[
                pos2(990.0, -10.0),
                pos2(1020.0, -10.0),
                pos2(1020.0, 10.0),
                pos2(990.0, 10.0),
            ],
        );
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        let [InkOutcome::Selected(selection)] = out.outcomes.as_slice() else {
            panic!("expected selection")
        };
        assert!(selection.hits.contains(&SceneHit::Object(ink)));
        assert!(selection.hits.contains(&SceneHit::Topic(card)));
        assert_eq!(*document.sheet(), before);

        let out = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::ObjectEraser,
            InkSettings::default(),
            &[pos2(1005.0, 0.0)],
        );
        assert!(
            matches!(out.errors.as_slice(), [InkControllerError::Locked(members)] if members.contains(&BoardMember::Card(card)))
        );
        assert_eq!(*document.sheet(), before);
        assert!(controller.cancel());
        let out = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::PixelEraser,
            InkSettings::default(),
            &[pos2(1005.0, 0.0)],
        );
        assert!(
            matches!(out.errors.as_slice(), [InkControllerError::Locked(members)] if members.contains(&BoardMember::Card(card)))
        );
        assert_eq!(*document.sheet(), before);
    }

    #[test]
    fn pixel_erase_refuses_to_change_derived_clipping() {
        let mut document = Document::default();
        let id = document
            .add_canvas_object(stroke(&[(-100.0, 0.0), (200.0, 0.0)]))
            .unwrap();
        document
            .create_board_frame(
                "Clip",
                BoardRect::from_min_size(Vec2::new(0.0, -50.0), Vec2::new(100.0, 100.0)),
            )
            .unwrap();
        document
            .edit_sheet("Enable clipping", |sheet| {
                sheet.zones[0].clip = true;
                Ok(())
            })
            .unwrap();
        let before = document.sheet().clone();
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings {
            eraser_radius: 1.0,
            ..InkSettings::default()
        };
        let out = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::PixelEraser,
            settings,
            &[pos2(50.0, 0.0)],
        );
        assert_eq!(out.errors, vec![InkControllerError::ClipWouldChange(id)]);
        assert_eq!(*document.sheet(), before);
        assert!(controller.has_pending_gesture());
    }

    #[test]
    fn stale_scene_error_can_be_retried_without_losing_the_gesture() {
        let mut document = Document::default();
        let old_scene = scene(&document);
        let ink = document
            .add_canvas_object(stroke(&[(100.0, 100.0), (110.0, 100.0)]))
            .unwrap();
        let current_scene = scene(&document);
        let before = document.sheet().clone();
        let mut controller = InkController::default();
        let out = mouse_gesture(
            &mut controller,
            &mut document,
            &old_scene,
            InkMode::ObjectEraser,
            InkSettings::default(),
            &[pos2(105.0, 100.0)],
        );
        assert_eq!(out.errors, vec![InkControllerError::StaleScene(ink)]);
        assert_eq!(*document.sheet(), before);
        assert_eq!(controller.pending_samples().len(), 1);
        assert_eq!(
            controller.retry(&mut document, &current_scene).unwrap(),
            InkOutcome::ObjectErased(vec![ink])
        );
        assert!(!controller.has_pending_gesture());
        assert!(document.undo());
        assert_eq!(*document.sheet(), before);
    }

    #[test]
    fn pixel_erase_sheet_limit_fails_atomically() {
        let mut workbook = Workbook::default();
        let sheet = workbook.active_mut();
        for i in 0..MAX_CANVAS_OBJECTS - 1 {
            sheet
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    BoardRect::from_min_size(
                        Vec2::new(500.0 + (i % 5) as f32, 500.0),
                        Vec2::new(1.0, 1.0),
                    ),
                ))
                .unwrap();
        }
        let id = sheet
            .add_canvas_object(stroke(&[(100.0, 100.0), (110.0, 100.0)]))
            .unwrap();
        let mut document = Document::new(workbook);
        let scene = scene(&document);
        let mut controller = InkController::default();
        let settings = InkSettings {
            eraser_radius: 1.0,
            ..InkSettings::default()
        };
        let out = mouse_gesture(
            &mut controller,
            &mut document,
            &scene,
            InkMode::PixelEraser,
            settings,
            &[pos2(105.0, 100.0)],
        );
        assert_eq!(
            out.errors,
            vec![InkControllerError::Limit {
                what: "canvas object count",
                limit: MAX_CANVAS_OBJECTS
            }]
        );
        assert_eq!(document.sheet().canvas.objects.len(), MAX_CANVAS_OBJECTS);
        assert!(document.sheet().canvas.object(id).is_some());
        assert!(!document.can_undo());
        assert!(controller.has_pending_gesture());
    }

    #[test]
    fn erasers_do_not_miss_small_hits_on_long_strokes() {
        for mode in [InkMode::ObjectEraser, InkMode::PixelEraser] {
            let mut document = Document::default();
            let mut line = stroke(&[(-1_000_000.0, 0.0), (0.03, 0.0)]);
            if let CanvasObjectKind::Ink(ink) = &mut line.kind {
                ink.width = 0.1;
            }
            let id = document.add_canvas_object(line).unwrap();
            let scene = scene(&document);
            let mut controller = InkController::default();
            let out = mouse_gesture(
                &mut controller,
                &mut document,
                &scene,
                mode,
                InkSettings {
                    eraser_radius: 0.0,
                    ..InkSettings::default()
                },
                &[pos2(0.07, 0.0)],
            );
            assert!(out.errors.is_empty(), "{:?}", out.errors);
            match mode {
                InkMode::ObjectEraser => {
                    assert_eq!(out.outcomes, vec![InkOutcome::ObjectErased(vec![id])])
                }
                InkMode::PixelEraser => assert!(
                    matches!(out.outcomes.as_slice(), [InkOutcome::PixelErased { originals, .. }] if originals == &[id])
                ),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn lasso_does_not_miss_a_small_hit_on_a_long_stroke() {
        let mut document = Document::default();
        let mut line = stroke(&[(-1_000_000.0, 0.0), (0.03, 0.0)]);
        if let CanvasObjectKind::Ink(ink) = &mut line.kind {
            ink.width = 0.1;
        }
        let id = document.add_canvas_object(line).unwrap();
        let scene = scene(&document);
        let out = mouse_gesture(
            &mut InkController::default(),
            &mut document,
            &scene,
            InkMode::Lasso,
            InkSettings::default(),
            &[
                pos2(0.069, -0.005),
                pos2(0.071, -0.005),
                pos2(0.071, 0.005),
                pos2(0.069, 0.005),
            ],
        );
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        assert!(matches!(
            out.outcomes.as_slice(),
            [InkOutcome::Selected(selection)]
                if selection.hits.contains(&SceneHit::Object(id)) && !selection.additive
        ));
    }
}
