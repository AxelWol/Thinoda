//! yMind application: document management, chrome and OS integration on top of the reusable crates.

pub mod ai;
pub mod app;
pub mod doc;
pub mod i18n;
mod structure_picker;
mod template_gallery;
pub mod ui;

pub use app::{Startup, YMindApp, start_services};

impl eframe::App for YMindApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn on_exit(&mut self) {
        self.shutdown();
    }
}
