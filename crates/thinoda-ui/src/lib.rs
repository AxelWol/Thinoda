//! # thinoda-ui
//!
//! A small, reusable design system on top of [`egui`] used by Thinoda and intended for
//! companion apps. It provides:
//!
//! * [`tokens`] – semantic color palettes (light/dark), radii and spacing.
//! * [`theme`] – installs the Thinoda look into an `egui::Context` and follows the OS
//!   light/dark preference.
//! * [`widgets`] – icon buttons, toolbar buttons, segmented tabs, toggles, color swatches,
//!   property rows, sections, chips and search fields.
//! * [`toast`] – non-blocking notifications.
//! * [`palette`] – a fuzzy command palette.
//! * [`dialog`] – modal confirm dialogs, the shared resizable dialog frame
//!   and the icon tab bar.
//! * [`fonts`] – system symbol-font fallbacks.
//! * [`icons`] – glyph constants available in egui's built-in fonts.

pub mod date_picker;
pub mod dialog;
pub mod fonts;
pub mod icons;
pub mod palette;
pub mod ribbon;
pub mod theme;
pub mod toast;
pub mod tokens;
pub mod widgets;

pub use date_picker::{CalendarDate, CalendarLabels, DateFieldResponse, date_field, days_in_month};
pub use dialog::{
    Dialog, DialogButton, DialogLabels, DialogResponse, DialogState, confirm_dialog,
    confirm_dialog_with_labels, tab_bar,
};
pub use palette::{Command, CommandPalette, fuzzy_score};
pub use theme::{ThemeMode, install};
pub use toast::{ToastKind, Toasts};
pub use tokens::{Palette, Tokens};
