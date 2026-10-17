//! Thinoda application: document management, chrome and OS integration on top of the reusable crates.

pub mod ai;
pub mod app;
pub mod doc;
mod help;
pub mod i18n;
mod labels;
mod map_shot;
mod quick_styles;
mod shortcut_browser;
mod structure_picker;
mod template_gallery;
mod theme_editor;
pub mod ui;

pub use app::{Startup, ThinodaApp, start_services};

impl eframe::App for ThinodaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn on_exit(&mut self) {
        self.shutdown();
    }
}
