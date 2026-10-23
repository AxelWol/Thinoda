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
//! * XMind keyboard shortcuts via [`Action`]

mod actions;
#[cfg(test)]
mod font_tests;
mod measure;
mod relationship_editing;
mod render;
mod topic_drag;
#[cfg(test)]
mod topic_ordering_tests;
mod view;

pub use actions::{Action, FloatingArrangement, apply_action};
pub use measure::EguiMeasure;
pub use render::{PaintOptions, Xf, color32, paint_map, paint_marker_colored, paint_thumbnail};
pub use view::{
    AddControlLabels, CanvasOutput, MapCanvas, MapView, NewTopicTitles, ObjectSelection,
    protect_ime_candidate_keys, shortcut_bindings,
};
