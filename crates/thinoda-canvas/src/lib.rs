//! # thinoda-canvas
//!
//! The interactive mind-map canvas for egui (rendered on the GPU through eframe's wgpu
//! backend). It draws a [`thinoda_core::Layout`] and turns pointer/keyboard input into
//! document edits:
//!
//! * pan (scroll / middle- or right-drag / Space+drag), zoom (Ctrl+wheel, pinch)
//! * click / Ctrl+click / rubber-band selection
//! * inline title editing (double-click, F2, Space, or just start typing)
//! * drag & drop to re-parent topics or create floating topics
//! * collapse badges, To-Do checkboxes, link icons
//! * relationship creation mode
//! * pressure-optional pen and highlighter, object/pixel erasers, and lasso
//! * XMind keyboard shortcuts via [`Action`]
//!
//! Ink tools use persistent [`MapView`] state: select a [`BoardTool`] and
//! configure [`MapView::ink_settings`] before showing [`MapCanvas`]. Inspect
//! [`CanvasOutput::ink_errors`] every frame for structured rejected samples
//! and failures; [`CanvasOutput::ink_outcomes`] reports completed/canceled
//! gestures. A failed stroke remains available through
//! [`MapView::pending_ink_samples`] until successfully retried or explicitly
//! canceled. Enter retries released gestures and Escape cancels them when the
//! canvas owns keyboard focus; hosts can also call [`MapView::retry_ink`],
//! [`MapView::retry_ink_with_finish_options`], or [`MapView::cancel_ink`].
//! Interrupted pointer input is retained rather than implicitly discarded.
//! Hosts should treat pending gestures and unacknowledged rejected samples as
//! unsaved input, and prevent save/export, sheet resets, and close operations
//! from omitting them. When a canvas is hidden, call [`MapView::suspend_ink`]
//! with its document and owning viewport's context, and retain its structured
//! errors. Continue calling it while [`MapView::needs_ink_input_tracking`]
//! is true so later pointer releases cannot leave input stuck. Suspension
//! preserves this frame's owned samples and releases the gesture for explicit
//! retry or cancellation; later input belongs to the new UI owner. The
//! samples and captured settings are available for a host-provided recovery UI
//! or diagnostic copy.

mod actions;
#[cfg(test)]
mod font_tests;
pub mod ink_controller;
pub mod ink_input;
mod measure;
pub mod minimap;
mod relationship_editing;
mod render;
mod topic_drag;
#[cfg(test)]
mod topic_ordering_tests;
mod view;

pub use actions::{Action, BoardAction, FloatingArrangement, apply_action, apply_board_action};
pub use measure::EguiMeasure;
pub use minimap::MinimapLabels;
pub use render::{
    PaintOptions, Xf, automatic_board_stroke, color32, paint_map, paint_marker_colored,
    paint_thumbnail, resource_preview_texture,
};
pub use view::{
    AddControlLabels, BoardLabels, BoardTool, CanvasOutput, MapCanvas, MapView, NewTopicTitles,
    ObjectSelection, TitleEditContinuation, protect_ime_candidate_keys, shortcut_bindings,
};
