//! # ymind-canvas
//!
//! The interactive mind-map canvas for egui (rendered on the GPU through eframe's wgpu
//! backend). It draws a [`ymind_core::Layout`] and turns pointer/keyboard input into
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
mod measure;
mod render;
mod view;

pub use actions::{Action, apply_action};
pub use measure::EguiMeasure;
pub use render::{color32, paint_map};
pub use view::{CanvasOutput, MapCanvas, MapView, NewTopicTitles};
